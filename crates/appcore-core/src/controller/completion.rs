// =============================================================================
//        #######
//     ###       ###     F: completion.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded process-local recovery between command effects and receipt publication.

use super::*;
use crate::idempotency::{IdempotencyRecord, IdempotencyStatus};
use std::collections::HashMap;
use std::io::{self, Write};

const MAX_COMPLETIONS: usize = 64;
const MAX_COMPLETION_BYTES: usize = 1024 * 1024;

#[derive(Default)]
pub(super) struct CompletionBuffer {
    entries: HashMap<String, Completion>,
}

struct Completion {
    request_hash: String,
    response_body: Option<String>,
    trace: Option<crate::TraceContext>,
}

impl RuntimeController {
    /// Counts outcomes retained after a receipt failure and recoverable by retry.
    ///
    /// This excludes executing handlers and reservations restored after restart.
    pub fn recoverable_command_completions(&self) -> usize {
        self.completions
            .lock()
            .entries
            .values()
            .filter(|entry| entry.response_body.is_some())
            .count()
    }

    /// Finalizes a restarted pending reservation using application-owned evidence.
    ///
    /// The trusted caller must first reconcile the external effect and provide its
    /// authoritative result. This method never invokes the handler or infers an
    /// outcome. Already resolved, missing, mismatched and active reservations fail.
    /// A receipt failure retains the supplied result for ordinary identical retry.
    pub fn recover_pending_command(
        &self,
        command: &CommandEnvelope,
        result: CommandResult,
    ) -> RuntimeResult<CommandResult> {
        let _activity = self
            .admit_dispatch(command)
            .map_err(|_| RuntimeError::CommandRejected)?;
        let key = command
            .idempotency_key
            .as_deref()
            .ok_or(RuntimeError::InvalidIdempotencyKey { reason: "missing" })?;
        let request_hash = hash_command(command);
        let body = encode_result(&result)?;
        {
            let store = self.idempotency.lock();
            let mut completions = self.completions.lock();
            let record = store
                .get(key)?
                .ok_or_else(|| completion_error("recover_completion", "reservation missing"))?;
            check_hash(key, &record.request_hash, &request_hash)?;
            if !matches!(record.status, IdempotencyStatus::Pending)
                || completions.entries.contains_key(key)
            {
                return Err(RuntimeError::IdempotencyPending {
                    key: key.to_string(),
                });
            }
            if completions.entries.len() >= MAX_COMPLETIONS {
                return Err(completion_error(
                    "admit_completion",
                    "completion capacity exhausted",
                ));
            }
            completions.entries.insert(
                key.to_string(),
                Completion {
                    request_hash,
                    response_body: Some(body),
                    trace: command.trace.clone(),
                },
            );
        }
        // Reservation is now completion-owned; racing dispatch can only finalize it.
        match self.reserve_idempotency(command)? {
            Some(result) => result,
            None => Err(completion_error(
                "recover_completion",
                "completion unavailable",
            )),
        }
    }

    pub(super) fn reserve_completion(
        &self,
        command: &CommandEnvelope,
    ) -> RuntimeResult<Option<RuntimeResult<CommandResult>>> {
        let Some(key) = command.idempotency_key.as_deref() else {
            return Ok(None);
        };
        let request_hash = hash_command(command);
        let mut store = self.idempotency.lock();
        let mut completions = self.completions.lock();
        if let Some(completion) = completions.entries.get(key) {
            check_hash(key, &completion.request_hash, &request_hash)?;
            let Some(body) = &completion.response_body else {
                return Err(RuntimeError::IdempotencyPending {
                    key: key.to_string(),
                });
            };
            let result = decode_result(body)?;
            store.insert(resolved_record(key, request_hash, body.clone()))?;
            let publication = CommandEnvelope {
                command_name: command.command_name.clone(),
                command_id: command.command_id.clone(),
                app_id: command.app_id.clone(),
                node_id: command.node_id.clone(),
                issued_at_ms: command.issued_at_ms,
                idempotency_key: command.idempotency_key.clone(),
                payload: Vec::new(),
                trace: completion.trace.clone(),
            };
            self.emit_events_and_audit(&publication, &Ok(result.clone()));
            completions.entries.remove(key);
            return Ok(Some(Ok(result)));
        }
        if let Some(record) = store.get(key)? {
            check_hash(key, &record.request_hash, &request_hash)?;
            match record.status {
                IdempotencyStatus::Pending => {
                    return Err(RuntimeError::IdempotencyPending {
                        key: key.to_string(),
                    })
                }
                IdempotencyStatus::Resolved {
                    response_status,
                    response_body,
                } => {
                    if response_status < 400 {
                        let result = if response_body.is_empty() {
                            CommandResult::accepted(Vec::new())
                        } else {
                            decode_result(&response_body)?
                        };
                        self.record_audit(command, &Ok(result.clone()));
                        return Ok(Some(Ok(result)));
                    }
                    store.remove(key)?;
                }
            }
        }
        if completions.entries.len() >= MAX_COMPLETIONS {
            return Err(completion_error(
                "admit_completion",
                "completion capacity exhausted",
            ));
        }
        store.insert(IdempotencyRecord {
            key: key.to_string(),
            request_hash: request_hash.clone(),
            status: IdempotencyStatus::Pending,
            created_at_ms: now_ms(),
        })?;
        completions.entries.insert(
            key.to_string(),
            Completion {
                request_hash,
                response_body: None,
                trace: command.trace.clone(),
            },
        );
        Ok(None)
    }

    /// Persists an outcome before emitting events, retaining failed completions.
    ///
    /// Call after a successful `pre_dispatch` reservation. Repeating finalization
    /// for an already resolved result does not emit its events again. At most 64
    /// active keyed completions of 1 MiB each are retained. Oversized results leave
    /// the reservation pending for application reconciliation. Event delivery is
    /// still bounded process-local publication, not a transactional durable outbox.
    pub fn post_dispatch(
        &self,
        command: &CommandEnvelope,
        dispatch_result: &RuntimeResult<CommandResult>,
    ) -> RuntimeResult<()> {
        let Some(key) = command.idempotency_key.as_deref() else {
            self.emit_events_and_audit(command, dispatch_result);
            return Ok(());
        };
        let mut store = self.idempotency.lock();
        let mut completions = self.completions.lock();
        let request_hash = hash_command(command);
        if let Some(completion) = completions.entries.get(key) {
            check_hash(key, &completion.request_hash, &request_hash)?;
        }
        if let Ok(result) = dispatch_result {
            let response_body = match encode_result(result) {
                Ok(body) => body,
                Err(error) => {
                    // The handler completed but its oversized outcome cannot be retained.
                    // Keep the durable reservation; release the local recovery capacity.
                    completions.entries.remove(key);
                    self.record_audit(command, &Err(error.clone()));
                    return Err(error);
                }
            };
            if let Some(completion) = completions.entries.get_mut(key) {
                check_hash(key, &completion.request_hash, &request_hash)?;
                if completion
                    .response_body
                    .as_ref()
                    .is_some_and(|body| body != &response_body)
                {
                    return Err(RuntimeError::IdempotencyConflict {
                        key: key.to_string(),
                    });
                }
                completion.response_body = Some(response_body.clone());
            } else {
                let record = store.get(key)?.ok_or_else(|| {
                    completion_error("finalize_completion", "reservation missing")
                })?;
                check_hash(key, &record.request_hash, &request_hash)?;
                if let IdempotencyStatus::Resolved {
                    response_body: existing,
                    ..
                } = record.status
                {
                    if existing != response_body {
                        return Err(RuntimeError::IdempotencyConflict {
                            key: key.to_string(),
                        });
                    }
                    return Ok(());
                }
                // A restarted pending reservation requires explicit application evidence.
                return Err(RuntimeError::IdempotencyPending {
                    key: key.to_string(),
                });
            }
            if let Err(error) = store.insert(resolved_record(key, request_hash, response_body)) {
                self.record_audit(command, &Err(error.clone()));
                return Err(error);
            }
        } else {
            let record = store
                .get(key)?
                .ok_or_else(|| completion_error("finalize_completion", "reservation missing"))?;
            check_hash(key, &record.request_hash, &request_hash)?;
            if !matches!(record.status, IdempotencyStatus::Pending) {
                return Err(RuntimeError::IdempotencyConflict {
                    key: key.to_string(),
                });
            }
            store.remove(key)?;
        }
        self.emit_events_and_audit(command, dispatch_result);
        completions.entries.remove(key);
        Ok(())
    }
}

fn check_hash(key: &str, stored: &str, expected: &str) -> RuntimeResult<()> {
    if !stored.starts_with("command-v2:") {
        return Err(completion_error(
            "validate_request_domain",
            "NO MORE SUPPORTED PLEASE UPDATE",
        ));
    }
    if stored != expected {
        return Err(RuntimeError::IdempotencyConflict {
            key: key.to_string(),
        });
    }
    Ok(())
}

fn resolved_record(key: &str, request_hash: String, response_body: String) -> IdempotencyRecord {
    IdempotencyRecord {
        key: key.to_string(),
        request_hash,
        status: IdempotencyStatus::Resolved {
            response_status: 200,
            response_body,
        },
        created_at_ms: now_ms(),
    }
}

fn decode_result(body: &str) -> RuntimeResult<CommandResult> {
    serde_json::from_str(body)
        .map_err(|_| completion_error("deserialize_replay", "invalid completion result"))
}

fn completion_error(operation: &'static str, message: &str) -> RuntimeError {
    RuntimeError::IdempotencyStoreIo {
        operation,
        message: message.to_string(),
    }
}

struct BoundedResult(Vec<u8>);

impl Write for BoundedResult {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.0.len().saturating_add(bytes.len()) > MAX_COMPLETION_BYTES {
            return Err(io::Error::other("completion result limit exceeded"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encode_result(result: &CommandResult) -> RuntimeResult<String> {
    let mut writer = BoundedResult(Vec::new());
    serde_json::to_writer(&mut writer, result).map_err(|_| {
        completion_error(
            "serialize_response",
            "completion result limit or encoding failure",
        )
    })?;
    String::from_utf8(writer.0)
        .map_err(|_| completion_error("serialize_response", "invalid completion encoding"))
}

// Key uniqueness is store-wide. App identity, command name and payload bind its
// semantic operation; transport IDs, timestamps, trace and origin node do not.
fn hash_command(command: &CommandEnvelope) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"appcore.command.idempotency.v2\0");
    for field in [
        command.app_id.as_str().as_bytes(),
        command.command_name.as_str().as_bytes(),
        command.payload.as_slice(),
    ] {
        hasher.update((field.len() as u64).to_be_bytes());
        hasher.update(field);
    }
    let result = hasher.finalize();
    let mut hex = String::from("command-v2:");
    for byte in result {
        hex.push_str(&format!("{:02x}", byte));
    }
    hex
}
