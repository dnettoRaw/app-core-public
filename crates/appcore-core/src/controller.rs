// =============================================================================
//        #######
//     ###       ###     F: controller.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Minimal runtime controller for lifecycle mutation and command dispatch delegation.

use crate::audit::{AuditCategory, AuditEntry, AuditOutcome, AuditRecord};
use crate::context::RuntimeContext;
use crate::envelope::CommandEnvelope;
use crate::error::{RuntimeError, RuntimeResult};
use crate::handler::CommandResult;
use crate::idempotency::{
    IdempotencyRecord, IdempotencyStatus, IdempotencyStore, InMemoryIdempotencyStore,
};
use crate::lifecycle::{RuntimeLifecycle, RuntimeLifecycleEvent, RuntimeLifecycleState};
use crate::runtime::RuntimeInstance;
use parking_lot::Mutex;
use std::sync::Arc;

/// Coordinates lifecycle, idempotency, command dispatch, events and audit.
pub struct RuntimeController {
    instance: Arc<RuntimeInstance>,
    idempotency: Mutex<Box<dyn IdempotencyStore + Send + Sync>>,
}

impl RuntimeController {
    /// Creates a controller with process-local idempotency.
    pub fn new(instance: RuntimeInstance) -> Self {
        Self {
            instance: Arc::new(instance),
            idempotency: Mutex::new(Box::new(InMemoryIdempotencyStore::new())),
        }
    }

    /// Creates a controller with an explicit idempotency store.
    pub fn with_idempotency_store(
        instance: RuntimeInstance,
        idempotency: Box<dyn IdempotencyStore + Send + Sync>,
    ) -> Self {
        Self {
            instance: Arc::new(instance),
            idempotency: Mutex::new(idempotency),
        }
    }

    /// Returns the hosted immutable Runtime instance.
    pub fn instance(&self) -> &RuntimeInstance {
        &self.instance
    }

    /// Returns a shared reference-counted Runtime instance.
    pub fn instance_arc(&self) -> Arc<RuntimeInstance> {
        self.instance.clone()
    }

    /// Returns the process lifecycle.
    pub fn lifecycle(&self) -> &RuntimeLifecycle {
        self.instance.lifecycle()
    }

    /// Returns the number of active idempotency records.
    pub fn idempotency_len(&self) -> usize {
        self.idempotency.lock().len()
    }

    /// Reports whether an idempotency key has an active record.
    pub fn idempotency_contains(&self, key: &str) -> RuntimeResult<bool> {
        Ok(self.idempotency.lock().get(key)?.is_some())
    }

    /// Applies one process lifecycle event.
    pub fn apply_lifecycle_event(
        &mut self,
        event: RuntimeLifecycleEvent,
    ) -> RuntimeResult<RuntimeLifecycleState> {
        self.instance.lifecycle().apply(event)
    }

    /// Runs the complete command dispatch transaction.
    pub fn dispatch_command(
        &mut self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        match self.pre_dispatch(command)? {
            Some(Ok(replay)) => Ok(replay),
            Some(Err(err)) => Err(err),
            None => {
                let result = self.instance.dispatch_command(command, context);
                self.post_dispatch(command, &result)?;
                result
            }
        }
    }

    /// Performs lifecycle and idempotency checks before handler execution.
    pub fn pre_dispatch(
        &self,
        command: &CommandEnvelope,
    ) -> RuntimeResult<Option<RuntimeResult<CommandResult>>> {
        if let Some(rejection) = self.check_lifecycle_readiness() {
            let res = Ok(rejection);
            self.record_audit(command, &res);
            return Ok(Some(res));
        }

        if let Some(key) = command.idempotency_key.as_deref() {
            let mut store = self.idempotency.lock();
            if let Some(record) = store.get(key)? {
                let payload_hash = hash_payload(&command.payload);
                if record.request_hash != payload_hash {
                    let err = RuntimeError::IdempotencyConflict {
                        key: key.to_string(),
                    };
                    self.record_audit(command, &Err(err.clone()));
                    return Err(err);
                }
                match record.status {
                    IdempotencyStatus::Pending => {
                        let err = RuntimeError::IdempotencyPending {
                            key: key.to_string(),
                        };
                        self.record_audit(command, &Err(err.clone()));
                        return Err(err);
                    }
                    IdempotencyStatus::Resolved {
                        response_status,
                        ref response_body,
                    } => {
                        if response_status >= 400 {
                            store.remove(key)?;
                            return Ok(None);
                        }
                        let result = if response_body.is_empty() {
                            CommandResult::accepted(Vec::new())
                        } else {
                            serde_json::from_str::<CommandResult>(response_body).map_err(|e| {
                                RuntimeError::IdempotencyStoreIo {
                                    operation: "deserialize_replay",
                                    message: e.to_string(),
                                }
                            })?
                        };
                        let res = Ok(result);
                        self.record_audit(command, &res);
                        return Ok(Some(res));
                    }
                }
            }

            // Insert Pending record
            let payload_hash = hash_payload(&command.payload);
            let record = IdempotencyRecord {
                key: key.to_string(),
                request_hash: payload_hash,
                status: IdempotencyStatus::Pending,
                created_at_ms: now_ms(),
            };
            store.insert(record)?;
        }

        Ok(None)
    }

    /// Persists command outcome and emits accepted events after handler execution.
    pub fn post_dispatch(
        &self,
        command: &CommandEnvelope,
        dispatch_result: &RuntimeResult<CommandResult>,
    ) -> RuntimeResult<()> {
        if let Some(key) = command.idempotency_key.as_ref() {
            let mut store = self.idempotency.lock();
            match dispatch_result {
                Ok(result) => {
                    let response_body = serde_json::to_string(result).map_err(|e| {
                        RuntimeError::IdempotencyStoreIo {
                            operation: "serialize_response",
                            message: e.to_string(),
                        }
                    })?;
                    let record = IdempotencyRecord {
                        key: key.to_string(),
                        request_hash: hash_payload(&command.payload),
                        status: IdempotencyStatus::Resolved {
                            response_status: 200,
                            response_body,
                        },
                        created_at_ms: now_ms(),
                    };
                    store.insert(record)?;
                }
                Err(_) => {
                    store.remove(key)?;
                }
            }
        }

        self.emit_events_and_audit(command, dispatch_result);
        Ok(())
    }

    fn check_lifecycle_readiness(&self) -> Option<CommandResult> {
        match self.lifecycle().current() {
            RuntimeLifecycleState::Running | RuntimeLifecycleState::Degraded => None,
            RuntimeLifecycleState::Restricted => {
                Some(CommandResult::rejected("runtime is restricted"))
            }
            _ => Some(CommandResult::rejected("runtime is not ready")),
        }
    }

    fn emit_events_and_audit(
        &self,
        command: &CommandEnvelope,
        dispatch_result: &RuntimeResult<CommandResult>,
    ) {
        if let Ok(result) = dispatch_result {
            if result.is_accepted() {
                let events = result
                    .events()
                    .iter()
                    .cloned()
                    .map(|event| {
                        if event.trace.is_none() {
                            if let Some(trace) = &command.trace {
                                return event.with_trace(trace.clone());
                            }
                        }
                        event
                    })
                    .collect::<Vec<_>>();
                for event in &events {
                    let completed_at_ms = now_ms();
                    self.instance.audit_log().push_entry(
                        AuditEntry::new(
                            AuditCategory::Event,
                            event.event_id.clone(),
                            event.event_name.as_str(),
                            event.occurred_at_ms,
                            completed_at_ms,
                            AuditOutcome::Accepted,
                        )
                        .with_runtime_scope(&event.app_id, &event.node_id)
                        .with_trace(event.trace.clone()),
                    );
                }
                self.instance.event_bus().emit_many(events);
            }
        }
        self.record_audit(command, dispatch_result);
    }

    fn record_audit(
        &self,
        command: &CommandEnvelope,
        dispatch_result: &RuntimeResult<CommandResult>,
    ) {
        let record = match dispatch_result {
            Ok(result) => AuditRecord {
                command_id: command.command_id.clone(),
                command_name: command.command_name.clone(),
                app_id: command.app_id.clone(),
                node_id: command.node_id.clone(),
                timestamp_ms: command.issued_at_ms,
                outcome: if result.is_accepted() {
                    AuditOutcome::Accepted
                } else {
                    AuditOutcome::Rejected
                },
                message: result.message().map(|msg| msg.to_string()),
                trace: command.trace.clone(),
            },
            Err(error) => AuditRecord {
                command_id: command.command_id.clone(),
                command_name: command.command_name.clone(),
                app_id: command.app_id.clone(),
                node_id: command.node_id.clone(),
                timestamp_ms: command.issued_at_ms,
                outcome: AuditOutcome::Error,
                message: Some(format!("{error:?}")),
                trace: command.trace.clone(),
            },
        };

        self.instance.audit_log().push(record);
    }
}

fn hash_payload(payload: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(payload);
    let result = hasher.finalize();
    let mut hex = String::with_capacity(result.len() * 2);
    for byte in result {
        hex.push_str(&format!("{:02x}", byte));
    }
    hex
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod controller_tests;
