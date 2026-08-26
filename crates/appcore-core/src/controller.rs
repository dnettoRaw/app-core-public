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
use parking_lot::{Condvar, Mutex};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Coordinates lifecycle, idempotency, command dispatch, events and audit.
pub struct RuntimeController {
    instance: Arc<RuntimeInstance>,
    idempotency: Arc<Mutex<Box<dyn IdempotencyStore + Send + Sync>>>,
    activity: Arc<CommandActivity>,
}

impl Clone for RuntimeController {
    fn clone(&self) -> Self {
        Self {
            instance: Arc::clone(&self.instance),
            idempotency: Arc::clone(&self.idempotency),
            activity: Arc::clone(&self.activity),
        }
    }
}

#[derive(Default)]
struct CommandActivity {
    state: Mutex<CommandActivityState>,
    idle: Condvar,
}

#[derive(Default)]
struct CommandActivityState {
    accepting: bool,
    inflight: usize,
}

struct CommandActivityGuard {
    activity: Arc<CommandActivity>,
}

impl Drop for CommandActivityGuard {
    fn drop(&mut self) {
        let mut state = self.activity.state.lock();
        state.inflight = state.inflight.saturating_sub(1);
        if state.inflight == 0 {
            self.activity.idle.notify_all();
        }
    }
}

impl RuntimeController {
    /// Creates a controller with process-local idempotency.
    pub fn new(instance: RuntimeInstance) -> Self {
        Self {
            instance: Arc::new(instance),
            idempotency: Arc::new(Mutex::new(Box::new(InMemoryIdempotencyStore::new()))),
            activity: Arc::new(CommandActivity::accepting()),
        }
    }

    /// Creates a controller with an explicit idempotency store.
    pub fn with_idempotency_store(
        instance: RuntimeInstance,
        idempotency: Box<dyn IdempotencyStore + Send + Sync>,
    ) -> Self {
        Self {
            instance: Arc::new(instance),
            idempotency: Arc::new(Mutex::new(idempotency)),
            activity: Arc::new(CommandActivity::accepting()),
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
        &self,
        event: RuntimeLifecycleEvent,
    ) -> RuntimeResult<RuntimeLifecycleState> {
        match event {
            RuntimeLifecycleEvent::ShutdownRequested => {
                let mut activity = self.activity.state.lock();
                let state = self.instance.lifecycle().apply(event)?;
                activity.accepting = false;
                Ok(state)
            }
            RuntimeLifecycleEvent::ShutdownCompleted => {
                if self.inflight_commands() != 0 {
                    return Err(RuntimeError::CommandRejected);
                }
                self.instance.lifecycle().apply(event)
            }
            _ => self.instance.lifecycle().apply(event),
        }
    }

    /// Runs the complete command dispatch transaction.
    pub fn dispatch_command(
        &self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        let _activity = match self.admit_dispatch(command) {
            Ok(activity) => activity,
            Err(rejection) => return Ok(rejection),
        };
        match self.reserve_idempotency(command)? {
            Some(Ok(replay)) => Ok(replay),
            Some(Err(err)) => Err(err),
            None => {
                let result = self.instance.dispatch_command(command, context);
                self.post_dispatch(command, &result)?;
                result
            }
        }
    }

    /// Returns the number of commands currently executing or finalizing.
    pub fn inflight_commands(&self) -> usize {
        self.activity.state.lock().inflight
    }

    /// Waits up to `timeout` for every admitted command to finish.
    ///
    /// Callers must request the shutdown lifecycle transition first so new
    /// commands cannot enter while the drain is in progress.
    pub fn wait_for_inflight(&self, timeout: Duration) -> bool {
        let started = Instant::now();
        let mut activity = self.activity.state.lock();
        while activity.inflight != 0 {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return false;
            }
            self.activity.idle.wait_for(&mut activity, remaining);
        }
        true
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

        self.reserve_idempotency(command)
    }

    fn reserve_idempotency(
        &self,
        command: &CommandEnvelope,
    ) -> RuntimeResult<Option<RuntimeResult<CommandResult>>> {
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

    fn admit_dispatch(
        &self,
        command: &CommandEnvelope,
    ) -> Result<CommandActivityGuard, CommandResult> {
        let mut activity = self.activity.state.lock();
        let rejection = if activity.accepting {
            self.check_lifecycle_readiness()
        } else {
            Some(CommandResult::rejected("runtime is not ready"))
        };
        if let Some(rejection) = rejection {
            let result = Ok(rejection.clone());
            drop(activity);
            self.record_audit(command, &result);
            return Err(rejection);
        }
        activity.inflight = activity.inflight.saturating_add(1);
        Ok(CommandActivityGuard {
            activity: Arc::clone(&self.activity),
        })
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

impl CommandActivity {
    fn accepting() -> Self {
        Self {
            state: Mutex::new(CommandActivityState {
                accepting: true,
                inflight: 0,
            }),
            idle: Condvar::new(),
        }
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
