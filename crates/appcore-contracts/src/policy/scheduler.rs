// =============================================================================
//        #######
//     ###       ###     F: scheduler.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Scheduler requirements declared by an application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulerRequirements {
    required: bool,
    max_concurrency: u32,
}

impl SchedulerRequirements {
    /// Creates scheduler requirements.
    pub fn new(required: bool, max_concurrency: u32) -> ContractResult<Self> {
        if required && max_concurrency == 0 {
            return Err(ContractError::InvalidValue {
                field: "scheduler.max_concurrency",
                reason: "must be greater than zero when a scheduler is required",
            });
        }
        Ok(Self {
            required,
            max_concurrency,
        })
    }

    /// Reports whether a scheduler is required.
    pub fn is_required(&self) -> bool {
        self.required
    }

    /// Returns the requested scheduler concurrency.
    pub fn max_concurrency(&self) -> u32 {
        self.max_concurrency
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        if self.required && self.max_concurrency == 0 {
            return Err(ContractError::InvalidValue {
                field: "scheduler.max_concurrency",
                reason: "must be greater than zero when a scheduler is required",
            });
        }
        Ok(())
    }
}
