// =============================================================================
//        #######
//     ###       ###     F: job.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Policy for jobs declared by an application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JobPolicy {
    enabled: bool,
    max_concurrency: u32,
    retry_limit: u32,
}

impl JobPolicy {
    /// Creates a job policy.
    pub fn new(enabled: bool, max_concurrency: u32, retry_limit: u32) -> ContractResult<Self> {
        if enabled && max_concurrency == 0 {
            return Err(ContractError::InvalidValue {
                field: "jobs.max_concurrency",
                reason: "must be greater than zero when jobs are enabled",
            });
        }
        Ok(Self {
            enabled,
            max_concurrency,
            retry_limit,
        })
    }

    /// Returns a policy that disables distributed jobs.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            max_concurrency: 0,
            retry_limit: 0,
        }
    }

    /// Reports whether jobs are enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the maximum number of concurrent jobs.
    pub fn max_concurrency(&self) -> u32 {
        self.max_concurrency
    }

    /// Returns the retry limit for failed jobs.
    pub fn retry_limit(&self) -> u32 {
        self.retry_limit
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        if self.enabled && self.max_concurrency == 0 {
            return Err(ContractError::InvalidValue {
                field: "jobs.max_concurrency",
                reason: "must be greater than zero when jobs are enabled",
            });
        }
        Ok(())
    }
}
