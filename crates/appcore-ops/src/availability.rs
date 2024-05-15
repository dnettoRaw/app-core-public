// =============================================================================
//        #######
//     ###       ###     F: availability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Stable Runtime liveness and readiness semantics.

use crate::HealthStatus;
use appcore_core::RuntimeOperationalMode;
use serde::{Deserialize, Serialize};

/// Coarse operational availability exposed to supervisors and routers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeAvailabilityState {
    /// Runtime can serve the work allowed by its configured mode.
    Ready,
    /// Runtime serves a reduced local workload while a dependency is impaired.
    Degraded,
    /// Runtime is alive but policy or security prevents normal traffic.
    Restricted,
    /// Runtime is alive and local-first reads may continue without coordination.
    Isolated,
    /// Runtime is not alive and accepts no traffic.
    Stopped,
}

/// Stable health projection for process, local, distributed, and write traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAvailabilityReport {
    /// Coarse availability state.
    pub state: RuntimeAvailabilityState,
    /// Process watchdog signal. False only after the Runtime stops.
    pub liveness: bool,
    /// Whether policy-authorized local queries can be served.
    pub local_readiness: bool,
    /// Whether work requiring cluster coordination can be served.
    pub distributed_readiness: bool,
    /// Whether state-changing commands can be accepted.
    pub write_readiness: bool,
}

impl RuntimeAvailabilityReport {
    /// Projects component health and operational mode into stable semantics.
    pub fn evaluate(health: HealthStatus, mode: RuntimeOperationalMode) -> Self {
        if health == HealthStatus::Stopped {
            return Self::stopped();
        }
        if health == HealthStatus::Restricted {
            return Self::restricted();
        }
        match mode {
            RuntimeOperationalMode::Isolated => Self {
                state: RuntimeAvailabilityState::Isolated,
                liveness: true,
                local_readiness: true,
                distributed_readiness: false,
                write_readiness: false,
            },
            RuntimeOperationalMode::Degraded => Self::degraded(true),
            RuntimeOperationalMode::Starting
            | RuntimeOperationalMode::Discovering
            | RuntimeOperationalMode::Syncing => Self::degraded(false),
            RuntimeOperationalMode::ReadOnly => Self::ready(false),
            RuntimeOperationalMode::ReadWrite if health == HealthStatus::Degraded => {
                Self::degraded(true)
            }
            RuntimeOperationalMode::ReadWrite => Self::ready(true),
        }
    }

    fn ready(writes: bool) -> Self {
        Self {
            state: RuntimeAvailabilityState::Ready,
            liveness: true,
            local_readiness: true,
            distributed_readiness: true,
            write_readiness: writes,
        }
    }

    fn degraded(local_ready: bool) -> Self {
        Self {
            state: RuntimeAvailabilityState::Degraded,
            liveness: true,
            local_readiness: local_ready,
            distributed_readiness: false,
            write_readiness: false,
        }
    }

    fn restricted() -> Self {
        Self {
            state: RuntimeAvailabilityState::Restricted,
            liveness: true,
            local_readiness: false,
            distributed_readiness: false,
            write_readiness: false,
        }
    }

    fn stopped() -> Self {
        Self {
            state: RuntimeAvailabilityState::Stopped,
            liveness: false,
            local_readiness: false,
            distributed_readiness: false,
            write_readiness: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_write_health_is_fully_ready() {
        let report = RuntimeAvailabilityReport::evaluate(
            HealthStatus::Healthy,
            RuntimeOperationalMode::ReadWrite,
        );
        assert_eq!(report.state, RuntimeAvailabilityState::Ready);
        assert!(report.liveness);
        assert!(report.local_readiness);
        assert!(report.distributed_readiness);
        assert!(report.write_readiness);
    }

    #[test]
    fn isolated_runtime_keeps_only_local_reads_ready() {
        let report = RuntimeAvailabilityReport::evaluate(
            HealthStatus::Healthy,
            RuntimeOperationalMode::Isolated,
        );
        assert_eq!(report.state, RuntimeAvailabilityState::Isolated);
        assert!(report.liveness);
        assert!(report.local_readiness);
        assert!(!report.distributed_readiness);
        assert!(!report.write_readiness);
    }

    #[test]
    fn restricted_and_stopped_states_fail_readiness() {
        let restricted = RuntimeAvailabilityReport::evaluate(
            HealthStatus::Restricted,
            RuntimeOperationalMode::ReadWrite,
        );
        let stopped = RuntimeAvailabilityReport::evaluate(
            HealthStatus::Stopped,
            RuntimeOperationalMode::ReadWrite,
        );
        assert!(restricted.liveness);
        assert!(!restricted.local_readiness);
        assert!(!stopped.liveness);
    }
}
