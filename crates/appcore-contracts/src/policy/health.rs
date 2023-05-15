// =============================================================================
//        #######
//     ###       ###     F: health.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Health timing required by an application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthRequirements {
    startup_grace_ms: u64,
    heartbeat_interval_ms: u64,
    failure_threshold: u32,
}

impl HealthRequirements {
    /// Creates health requirements.
    pub fn new(
        startup_grace_ms: u64,
        heartbeat_interval_ms: u64,
        failure_threshold: u32,
    ) -> ContractResult<Self> {
        if heartbeat_interval_ms == 0 || failure_threshold == 0 {
            return Err(ContractError::InvalidValue {
                field: "health",
                reason: "heartbeat interval and failure threshold must be greater than zero",
            });
        }
        Ok(Self {
            startup_grace_ms,
            heartbeat_interval_ms,
            failure_threshold,
        })
    }

    /// Returns startup grace time in milliseconds.
    pub fn startup_grace_ms(&self) -> u64 {
        self.startup_grace_ms
    }

    /// Returns heartbeat interval in milliseconds.
    pub fn heartbeat_interval_ms(&self) -> u64 {
        self.heartbeat_interval_ms
    }

    /// Returns consecutive failures tolerated before unhealthy state.
    pub fn failure_threshold(&self) -> u32 {
        self.failure_threshold
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        if self.heartbeat_interval_ms == 0 || self.failure_threshold == 0 {
            return Err(ContractError::InvalidValue {
                field: "health",
                reason: "heartbeat interval and failure threshold must be greater than zero",
            });
        }
        Ok(())
    }
}
