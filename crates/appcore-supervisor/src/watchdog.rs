// =============================================================================
//        #######
//     ###       ###     F: watchdog.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Lock-independent reconciliation progress watchdog.

use crate::{SupervisorError, SupervisorResult};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};

/// Default watchdog check interval.
pub const DEFAULT_WATCHDOG_CHECK_INTERVAL_MS: u64 = 1_000;
/// Default maximum interval without completed reconciliation.
pub const DEFAULT_WATCHDOG_STALL_TIMEOUT_MS: u64 = 15_000;

/// Watchdog policy supplied by the installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchdogConfig {
    /// Whether watchdog health affects Runtime health.
    pub enabled: bool,
    /// Independent watchdog evaluation interval.
    pub check_interval_ms: u64,
    /// Maximum interval without a completed reconciliation.
    pub stall_timeout_ms: u64,
}

impl WatchdogConfig {
    /// Validates safe watchdog bounds.
    pub fn validate(self) -> SupervisorResult<Self> {
        if self.check_interval_ms == 0 || self.stall_timeout_ms == 0 {
            return Err(SupervisorError::InvalidConfiguration(
                "watchdog intervals must be greater than zero".to_string(),
            ));
        }
        if self.enabled && self.stall_timeout_ms <= self.check_interval_ms {
            return Err(SupervisorError::InvalidConfiguration(
                "watchdog stall timeout must exceed its check interval".to_string(),
            ));
        }
        Ok(self)
    }
}

impl Default for WatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_ms: DEFAULT_WATCHDOG_CHECK_INTERVAL_MS,
            stall_timeout_ms: DEFAULT_WATCHDOG_STALL_TIMEOUT_MS,
        }
    }
}

/// Observable watchdog lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogState {
    /// No reconciliation cycle has completed yet.
    Starting,
    /// Reconciliation is completing inside the configured timeout.
    Healthy,
    /// A reconciliation cycle stopped making progress.
    Stalled,
    /// The watchdog itself encountered an unrecoverable failure.
    Failed,
    /// Runtime shutdown is in progress.
    Stopping,
}

impl WatchdogState {
    fn as_u8(self) -> u8 {
        match self {
            Self::Starting => 0,
            Self::Healthy => 1,
            Self::Stalled => 2,
            Self::Failed => 3,
            Self::Stopping => 4,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            1 => Self::Healthy,
            2 => Self::Stalled,
            3 => Self::Failed,
            4 => Self::Stopping,
            _ => Self::Starting,
        }
    }
}

/// Atomic watchdog state shared by reconcile, health, and watchdog threads.
pub struct SupervisorWatchdog {
    config: WatchdogConfig,
    created_at_ms: u64,
    last_reconcile_at_ms: AtomicU64,
    reconcile_sequence: AtomicU64,
    last_progress_at_ms: AtomicU64,
    state: AtomicU8,
}

impl SupervisorWatchdog {
    /// Creates a watchdog with validated installation policy.
    pub fn new(config: WatchdogConfig, created_at_ms: u64) -> SupervisorResult<Self> {
        Ok(Self::from_validated(config.validate()?, created_at_ms))
    }

    pub(crate) fn with_default(created_at_ms: u64) -> Self {
        Self::from_validated(WatchdogConfig::default(), created_at_ms)
    }

    fn from_validated(config: WatchdogConfig, created_at_ms: u64) -> Self {
        Self {
            config,
            created_at_ms,
            last_reconcile_at_ms: AtomicU64::new(0),
            reconcile_sequence: AtomicU64::new(0),
            last_progress_at_ms: AtomicU64::new(0),
            state: AtomicU8::new(WatchdogState::Starting.as_u8()),
        }
    }

    /// Records entry into one reconciliation cycle without taking a lock.
    pub fn record_reconcile_started(&self, timestamp_ms: u64) {
        self.last_reconcile_at_ms
            .store(timestamp_ms, Ordering::Release);
    }

    /// Records successful completion and returns the new sequence.
    pub fn record_reconcile_completed(&self, timestamp_ms: u64) -> u64 {
        self.last_reconcile_at_ms
            .store(timestamp_ms, Ordering::Release);
        self.last_progress_at_ms
            .store(timestamp_ms, Ordering::Release);
        let sequence = self
            .reconcile_sequence
            .fetch_add(1, Ordering::AcqRel)
            .saturating_add(1);
        if !matches!(
            self.state(),
            WatchdogState::Stopping | WatchdogState::Failed
        ) {
            self.set_state(WatchdogState::Healthy);
        }
        sequence
    }

    /// Evaluates progress and returns a state transition when one occurred.
    pub fn evaluate(&self, timestamp_ms: u64) -> Option<(WatchdogState, WatchdogState)> {
        let previous = self.state();
        if matches!(previous, WatchdogState::Stopping | WatchdogState::Failed) {
            return None;
        }
        let next = if !self.config.enabled {
            WatchdogState::Healthy
        } else if self.stalled_for_ms(timestamp_ms) > self.config.stall_timeout_ms {
            WatchdogState::Stalled
        } else if self.reconcile_sequence.load(Ordering::Acquire) == 0 {
            WatchdogState::Starting
        } else {
            WatchdogState::Healthy
        };
        if previous == next {
            return None;
        }
        self.set_state(next);
        Some((previous, next))
    }

    /// Marks watchdog shutdown without changing reconciliation counters.
    pub fn mark_stopping(&self) {
        self.set_state(WatchdogState::Stopping);
    }

    /// Marks an unrecoverable watchdog failure.
    pub fn mark_failed(&self) {
        self.set_state(WatchdogState::Failed);
    }

    /// Returns immutable watchdog policy.
    pub fn config(&self) -> WatchdogConfig {
        self.config
    }

    /// Returns the current atomic watchdog state.
    pub fn state(&self) -> WatchdogState {
        WatchdogState::from_u8(self.state.load(Ordering::Acquire))
    }

    /// Returns the most recent reconciliation start or completion time.
    pub fn last_reconcile_at_ms(&self) -> u64 {
        self.last_reconcile_at_ms.load(Ordering::Acquire)
    }

    /// Returns the number of completed reconciliation cycles.
    pub fn reconcile_sequence(&self) -> u64 {
        self.reconcile_sequence.load(Ordering::Acquire)
    }

    /// Returns the most recent completed reconciliation time.
    pub fn last_progress_at_ms(&self) -> u64 {
        self.last_progress_at_ms.load(Ordering::Acquire)
    }

    /// Returns elapsed time without a completed reconciliation cycle.
    pub fn stalled_for_ms(&self, timestamp_ms: u64) -> u64 {
        let progress = self.last_progress_at_ms();
        timestamp_ms.saturating_sub(if progress == 0 {
            self.created_at_ms
        } else {
            progress
        })
    }

    fn set_state(&self, state: WatchdogState) {
        self.state.store(state.as_u8(), Ordering::Release);
    }
}
