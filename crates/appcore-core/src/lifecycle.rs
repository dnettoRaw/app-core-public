// =============================================================================
//        #######
//     ###       ###     F: lifecycle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded/thread-safe runtime lifecycle contract built on top of StateMachine.

use crate::error::{RuntimeError, RuntimeResult};
use crate::ids::{EventName, StateName};
use crate::state::{StateMachine, StateTransition};
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
    machine: Mutex<StateMachine>,
}

impl Clone for RuntimeLifecycle {
    fn clone(&self) -> Self {
        let guard = self.machine.lock();
        Self {
            machine: Mutex::new(guard.clone()),
        }
    }
}

impl RuntimeLifecycleState {
    fn as_state_name(self) -> StateName {
        // appcore-norm: allow(clippy::unwrap_used) reason: enum mapping uses validated static state names
        StateName::new(match self {
            RuntimeLifecycleState::Booting => "Booting",
            RuntimeLifecycleState::LoadingConfig => "LoadingConfig",
            RuntimeLifecycleState::CheckingSecurity => "CheckingSecurity",
            RuntimeLifecycleState::OpeningStorage => "OpeningStorage",
            RuntimeLifecycleState::StartingApi => "StartingApi",
            RuntimeLifecycleState::Running => "Running",
            RuntimeLifecycleState::Degraded => "Degraded",
            RuntimeLifecycleState::Restricted => "Restricted",
            RuntimeLifecycleState::ShuttingDown => "ShuttingDown",
            RuntimeLifecycleState::Stopped => "Stopped",
        })
        .unwrap()
    }

    fn from_state_name(state: &StateName) -> RuntimeResult<Self> {
        match state.as_str() {
            "Booting" => Ok(Self::Booting),
            "LoadingConfig" => Ok(Self::LoadingConfig),
            "CheckingSecurity" => Ok(Self::CheckingSecurity),
            "OpeningStorage" => Ok(Self::OpeningStorage),
            "StartingApi" => Ok(Self::StartingApi),
            "Running" => Ok(Self::Running),
            "Degraded" => Ok(Self::Degraded),
            "Restricted" => Ok(Self::Restricted),
            "ShuttingDown" => Ok(Self::ShuttingDown),
            "Stopped" => Ok(Self::Stopped),
            _ => Err(RuntimeError::InvalidStateTransition),
        }
    }
}

impl RuntimeLifecycleEvent {
    fn as_event_name(self) -> EventName {
        // appcore-norm: allow(clippy::unwrap_used) reason: enum mapping uses validated static event names
        EventName::new(match self {
            RuntimeLifecycleEvent::ConfigLoaded => "ConfigLoaded",
            RuntimeLifecycleEvent::SecurityChecked => "SecurityChecked",
            RuntimeLifecycleEvent::StorageOpened => "StorageOpened",
            RuntimeLifecycleEvent::ApiStarted => "ApiStarted",
            RuntimeLifecycleEvent::DegradedDetected => "DegradedDetected",
            RuntimeLifecycleEvent::RestrictedDetected => "RestrictedDetected",
            RuntimeLifecycleEvent::ShutdownRequested => "ShutdownRequested",
            RuntimeLifecycleEvent::ShutdownCompleted => "ShutdownCompleted",
            RuntimeLifecycleEvent::RecoveryCompleted => "RecoveryCompleted",
        })
        .unwrap()
    }
}

impl RuntimeLifecycle {
    /// Creates a lifecycle in the booting state with all valid transitions.
    pub fn new() -> Self {
        // Máquina de estados explícita para o ciclo de vida do runtime.
        // Todas as transições são rígidas e validadas; qualquer transição inválida gera erro imediato
        // e impede o avanço de estado incorreto.
        let mut machine = StateMachine::new(RuntimeLifecycleState::Booting.as_state_name());
        let transitions = vec![
            (
                RuntimeLifecycleState::Booting,
                RuntimeLifecycleEvent::ConfigLoaded,
                RuntimeLifecycleState::CheckingSecurity,
            ),
            (
                RuntimeLifecycleState::CheckingSecurity,
                RuntimeLifecycleEvent::SecurityChecked,
                RuntimeLifecycleState::OpeningStorage,
            ),
            (
                RuntimeLifecycleState::OpeningStorage,
                RuntimeLifecycleEvent::StorageOpened,
                RuntimeLifecycleState::StartingApi,
            ),
            (
                RuntimeLifecycleState::StartingApi,
                RuntimeLifecycleEvent::ApiStarted,
                RuntimeLifecycleState::Running,
            ),
            (
                RuntimeLifecycleState::Running,
                RuntimeLifecycleEvent::DegradedDetected,
                RuntimeLifecycleState::Degraded,
            ),
            (
                RuntimeLifecycleState::Running,
                RuntimeLifecycleEvent::RestrictedDetected,
                RuntimeLifecycleState::Restricted,
            ),
            (
                RuntimeLifecycleState::Degraded,
                RuntimeLifecycleEvent::RecoveryCompleted,
                RuntimeLifecycleState::Running,
            ),
            (
                RuntimeLifecycleState::Restricted,
                RuntimeLifecycleEvent::RecoveryCompleted,
                RuntimeLifecycleState::Running,
            ),
            (
                RuntimeLifecycleState::Running,
                RuntimeLifecycleEvent::ShutdownRequested,
                RuntimeLifecycleState::ShuttingDown,
            ),
            (
                RuntimeLifecycleState::Degraded,
                RuntimeLifecycleEvent::ShutdownRequested,
                RuntimeLifecycleState::ShuttingDown,
            ),
            (
                RuntimeLifecycleState::Restricted,
                RuntimeLifecycleEvent::ShutdownRequested,
                RuntimeLifecycleState::ShuttingDown,
            ),
            (
                RuntimeLifecycleState::ShuttingDown,
                RuntimeLifecycleEvent::ShutdownCompleted,
                RuntimeLifecycleState::Stopped,
            ),
        ];

        for (from, event, to) in transitions {
            let _ = machine.add_transition(StateTransition {
                from: from.as_state_name(),
                event: event.as_event_name(),
                to: to.as_state_name(),
            });
        }

        Self {
            machine: Mutex::new(machine),
        }
    }

    /// Returns the current lifecycle state.
    pub fn current(&self) -> RuntimeLifecycleState {
        let guard = self.machine.lock();
        match RuntimeLifecycleState::from_state_name(guard.current()) {
            Ok(state) => state,
            Err(_) => RuntimeLifecycleState::Booting,
        }
    }

    /// Applies one lifecycle event and returns the resulting state.
    pub fn apply(&self, event: RuntimeLifecycleEvent) -> RuntimeResult<RuntimeLifecycleState> {
        let mut guard = self.machine.lock();
        let next = guard.apply(&event.as_event_name())?;
        RuntimeLifecycleState::from_state_name(next)
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
