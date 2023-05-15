// =============================================================================
//        #######
//     ###       ###     F: leadership.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Leadership behavior for one service.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LeadershipMode {
    /// The service never participates in leader election.
    #[default]
    Disabled,
    /// Leadership is useful but not required for all work.
    Preferred,
    /// Work is accepted only while this core holds the service lease.
    Required,
}

/// Service-scoped leadership requirement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeadershipRequirement {
    service_id: ServiceId,
    mode: LeadershipMode,
    lease_duration_ms: u64,
}

impl LeadershipRequirement {
    /// Creates a service-scoped leadership requirement.
    pub fn new(
        service_id: ServiceId,
        mode: LeadershipMode,
        lease_duration_ms: u64,
    ) -> ContractResult<Self> {
        if mode != LeadershipMode::Disabled && lease_duration_ms == 0 {
            return Err(ContractError::InvalidValue {
                field: "leadership.lease_duration_ms",
                reason: "must be greater than zero when leadership is enabled",
            });
        }
        Ok(Self {
            service_id,
            mode,
            lease_duration_ms,
        })
    }

    /// Returns the independently coordinated service.
    pub fn service_id(&self) -> &ServiceId {
        &self.service_id
    }

    /// Returns the leadership mode.
    pub fn mode(&self) -> LeadershipMode {
        self.mode
    }

    /// Returns the requested lease duration in milliseconds.
    pub fn lease_duration_ms(&self) -> u64 {
        self.lease_duration_ms
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        if self.mode != LeadershipMode::Disabled && self.lease_duration_ms == 0 {
            return Err(ContractError::InvalidValue {
                field: "leadership.lease_duration_ms",
                reason: "must be greater than zero when leadership is enabled",
            });
        }
        Ok(())
    }
}
