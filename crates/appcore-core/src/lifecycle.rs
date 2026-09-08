// =============================================================================
//        #######
//     ###       ###     F: lifecycle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded/thread-safe runtime lifecycle with a total enum transition function.

use crate::error::{RuntimeError, RuntimeResult};
use parking_lot::Mutex;

// NOTA: Estados de lifecycle estendidos como checking-identity, discovering-peers, readonly e syncing
// foram adiados para a versão v0.7 para manter a estabilidade do contrato de transições por enquanto.
/// Stable process lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeLifecycleState {
    /// Runtime process is booting.
    Booting,
    /// Runtime configuration is being loaded.
    LoadingConfig,
    /// Security configuration is being checked.
    CheckingSecurity,
    /// Storage boundaries are being opened.
    OpeningStorage,
    /// Runtime API boundaries are starting.
    StartingApi,
    /// Runtime is accepting declared work.
    Running,
    /// Runtime remains available with reduced guarantees.
    Degraded,
    /// Runtime accepts only explicitly permitted operations.
    Restricted,
    /// Runtime is performing graceful shutdown.
    ShuttingDown,
    /// Runtime has stopped.
    Stopped,
}

/// Event accepted by the Runtime process lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeLifecycleEvent {
    /// Configuration loading completed.
    ConfigLoaded,
    /// Security checks completed.
    SecurityChecked,
    /// Storage initialization completed.
    StorageOpened,
    /// API startup completed.
    ApiStarted,
    /// A degradable failure was observed.
    DegradedDetected,
    /// A restriction policy was activated.
    RestrictedDetected,
    /// Graceful shutdown was requested.
    ShutdownRequested,
    /// Graceful shutdown completed.
    ShutdownCompleted,
    /// A degraded or restricted condition recovered.
    RecoveryCompleted,
}

/// Thread-safe state machine for the Runtime process lifecycle.
#[derive(Debug)]
pub struct RuntimeLifecycle {
    state: Mutex<RuntimeLifecycleState>,
}

impl Clone for RuntimeLifecycle {
    fn clone(&self) -> Self {
        let state = *self.state.lock();
        Self {
            state: Mutex::new(state),
        }
    }
}

const fn next_state(
    state: RuntimeLifecycleState,
    event: RuntimeLifecycleEvent,
) -> Option<RuntimeLifecycleState> {
    match (state, event) {
        (RuntimeLifecycleState::Booting, RuntimeLifecycleEvent::ConfigLoaded) => {
            Some(RuntimeLifecycleState::CheckingSecurity)
        }
        (RuntimeLifecycleState::CheckingSecurity, RuntimeLifecycleEvent::SecurityChecked) => {
            Some(RuntimeLifecycleState::OpeningStorage)
        }
        (RuntimeLifecycleState::OpeningStorage, RuntimeLifecycleEvent::StorageOpened) => {
            Some(RuntimeLifecycleState::StartingApi)
        }
        (RuntimeLifecycleState::StartingApi, RuntimeLifecycleEvent::ApiStarted) => {
            Some(RuntimeLifecycleState::Running)
        }
        (RuntimeLifecycleState::Running, RuntimeLifecycleEvent::DegradedDetected) => {
            Some(RuntimeLifecycleState::Degraded)
        }
        (RuntimeLifecycleState::Running, RuntimeLifecycleEvent::RestrictedDetected) => {
            Some(RuntimeLifecycleState::Restricted)
        }
        (
            RuntimeLifecycleState::Degraded | RuntimeLifecycleState::Restricted,
            RuntimeLifecycleEvent::RecoveryCompleted,
        ) => Some(RuntimeLifecycleState::Running),
        (
            RuntimeLifecycleState::Running
            | RuntimeLifecycleState::Degraded
            | RuntimeLifecycleState::Restricted,
            RuntimeLifecycleEvent::ShutdownRequested,
        ) => Some(RuntimeLifecycleState::ShuttingDown),
        (RuntimeLifecycleState::ShuttingDown, RuntimeLifecycleEvent::ShutdownCompleted) => {
            Some(RuntimeLifecycleState::Stopped)
        }
        _ => None,
    }
}

impl RuntimeLifecycle {
    /// Creates a lifecycle in the booting state with all valid transitions.
    pub fn new() -> Self {
        Self {
            state: Mutex::new(RuntimeLifecycleState::Booting),
        }
    }

    /// Returns the current lifecycle state.
    pub fn current(&self) -> RuntimeLifecycleState {
        *self.state.lock()
    }

    /// Applies one lifecycle event and returns the resulting state.
    pub fn apply(&self, event: RuntimeLifecycleEvent) -> RuntimeResult<RuntimeLifecycleState> {
        let mut state = self.state.lock();
        let next = next_state(*state, event).ok_or(RuntimeError::InvalidStateTransition)?;
        *state = next;
        Ok(next)
    }

    /// Reports whether the lifecycle is in the normal running state.
    pub fn is_running(&self) -> bool {
        self.current() == RuntimeLifecycleState::Running
    }

    /// Reports whether shutdown has completed.
    pub fn is_stopped(&self) -> bool {
        self.current() == RuntimeLifecycleState::Stopped
    }

    /// Reports whether restricted operation is active.
    pub fn is_restricted(&self) -> bool {
        self.current() == RuntimeLifecycleState::Restricted
    }
}

impl Default for RuntimeLifecycle {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "lifecycle_tests.rs"]
mod tests;
