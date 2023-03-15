// =============================================================================
//        #######
//     ###       ###     F: policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Restart and shutdown policy.

use crate::{SupervisorError, SupervisorResult};
use std::time::Duration;

/// Conditions under which a managed service may restart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RestartMode {
    /// Never restart automatically.
    Never,
    /// Restart only after a failed health signal or unexpected exit.
    OnFailure,
    /// Restart after any unexpected service exit.
    Always,
}

/// Bounded restart and shutdown behavior for one service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartPolicy {
    /// Automatic restart condition.
    pub mode: RestartMode,
    /// Maximum restart attempts inside `restart_window`.
    pub restart_budget: u32,
    /// Sliding time window applied to the restart budget.
    pub restart_window: Duration,
    /// Base delay before a restart.
    pub backoff: Duration,
    /// Maximum random delay added to the base backoff.
    pub jitter: Duration,
    /// Cooperative shutdown deadline.
    pub shutdown_timeout: Duration,
}

impl RestartPolicy {
    /// Creates a policy that never restarts automatically.
    pub fn never() -> Self {
        Self {
            mode: RestartMode::Never,
            restart_budget: 0,
            restart_window: Duration::from_secs(600),
            backoff: Duration::ZERO,
            jitter: Duration::ZERO,
            shutdown_timeout: Duration::from_secs(10),
        }
    }

    /// Creates the default bounded on-failure policy.
    pub fn bounded(restart_budget: u32, restart_window: Duration) -> SupervisorResult<Self> {
        let policy = Self {
            mode: RestartMode::OnFailure,
            restart_budget,
            restart_window,
            backoff: Duration::from_millis(100),
            jitter: Duration::from_millis(50),
            shutdown_timeout: Duration::from_secs(10),
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Replaces backoff and jitter bounds.
    pub fn with_backoff(mut self, backoff: Duration, jitter: Duration) -> Self {
        self.backoff = backoff;
        self.jitter = jitter;
        self
    }

    /// Replaces the cooperative shutdown deadline.
    pub fn with_shutdown_timeout(mut self, shutdown_timeout: Duration) -> Self {
        self.shutdown_timeout = shutdown_timeout;
        self
    }

    /// Validates policy bounds.
    pub fn validate(&self) -> SupervisorResult<()> {
        if self.mode != RestartMode::Never
            && (self.restart_budget == 0 || self.restart_window.is_zero())
        {
            return Err(SupervisorError::InvalidConfiguration(
                "automatic restart requires a non-zero budget and window".to_string(),
            ));
        }
        if self.shutdown_timeout.is_zero() {
            return Err(SupervisorError::InvalidConfiguration(
                "shutdown_timeout must be greater than zero".to_string(),
            ));
        }
        Ok(())
    }
}

impl Default for RestartPolicy {
    fn default() -> Self {
        Self {
            mode: RestartMode::OnFailure,
            restart_budget: 5,
            restart_window: Duration::from_secs(600),
            backoff: Duration::from_millis(100),
            jitter: Duration::from_millis(50),
            shutdown_timeout: Duration::from_secs(10),
        }
    }
}
