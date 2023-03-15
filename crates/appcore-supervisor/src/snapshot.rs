// =============================================================================
//        #######
//     ###       ###     F: snapshot.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Immutable supervisor, service, watchdog, and executor diagnostics.

use crate::{
    RestartState, ServiceActivationState, ServiceHealth, ServiceRuntimeState, WatchdogState,
};

/// Immutable diagnostic snapshot for one managed service.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceSnapshot {
    /// Stable service identifier.
    pub name: String,
    /// Current supervisor health.
    pub health: ServiceHealth,
    /// Required service dependency identifiers.
    pub dependencies: Vec<String>,
    /// Human-readable dependency requirements in matching order.
    pub dependency_requirements: Vec<String>,
    /// Installation activation state.
    pub activation: ServiceActivationState,
    /// Whether lifecycle actions are enabled.
    pub enabled: bool,
    /// Whether deployment configuration exists.
    pub configured: bool,
    /// Whether the service may currently own its resource.
    pub running: bool,
    /// Concrete execution state.
    pub runtime_state: ServiceRuntimeState,
    /// Current restart state.
    pub restart_state: RestartState,
    /// Total restarts scheduled since supervisor creation.
    pub restart_count: u64,
    /// Whether an operator must explicitly recover the service.
    pub operator_required: bool,
    /// Whether automatic lifecycle actions are disabled.
    pub quarantined: bool,
    /// Whether this service affects overall Runtime health.
    pub critical: bool,
}

/// Immutable watchdog progress exposed to health consumers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchdogSnapshot {
    /// Current watchdog state.
    pub state: WatchdogState,
    /// Most recent reconciliation start or completion time.
    pub last_reconcile_at_ms: u64,
    /// Most recent completed reconciliation time.
    pub last_progress_at_ms: u64,
    /// Number of completed reconciliation cycles.
    pub reconcile_sequence: u64,
    /// Elapsed time without completed reconciliation.
    pub stalled_for_ms: u64,
    /// Whether every enabled critical service is ready or healthy.
    pub critical_services_healthy: bool,
    /// Whether watchdog enforcement is enabled.
    pub enabled: bool,
    /// Configured stall timeout.
    pub stall_timeout_ms: u64,
}

impl WatchdogSnapshot {
    /// Reports whether progress is currently trustworthy.
    pub fn is_healthy(&self) -> bool {
        !self.enabled || self.state == WatchdogState::Healthy
    }
}

/// Immutable bounded restart-executor diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestartExecutorSnapshot {
    /// Whether workers and queue remain available.
    pub healthy: bool,
    /// Number of restart actions currently queued or executing.
    pub pending: u64,
    /// Configured queue capacity.
    pub queue_capacity: usize,
    /// Configured worker count.
    pub worker_count: usize,
}

/// Complete result consumed by `appcore doctor` and diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorDiagnosis {
    /// Whether every required dependency exists and the graph is acyclic.
    pub graph_valid: bool,
    /// Controlled graph or policy issues.
    pub issues: Vec<String>,
    /// Deterministically ordered service snapshots.
    pub services: Vec<ServiceSnapshot>,
    /// Reconciliation progress and watchdog health.
    pub watchdog: WatchdogSnapshot,
    /// Restart worker and queue health.
    pub restart_executor: RestartExecutorSnapshot,
}
