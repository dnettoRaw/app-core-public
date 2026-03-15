// =============================================================================
//        #######
//     ###       ###     F: application_supervisor_health.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! External process-supervisor progress tracking.

use crate::supervisor::SupervisorHealthProgress;
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProgressState {
    Waiting,
    Advanced,
    Stalled,
    Failed,
}

#[derive(Debug, Default)]
pub(super) struct ProgressTracker {
    last_sequence: Option<u64>,
    last_progress_at_ms: u64,
    unchanged_since_ms: u64,
}

impl ProgressTracker {
    pub(super) fn observe(
        &mut self,
        progress: Option<SupervisorHealthProgress>,
        timestamp_ms: u64,
        stall_timeout: Duration,
    ) -> ProgressState {
        let Some(progress) = progress else {
            return self.no_progress(timestamp_ms, stall_timeout);
        };
        if matches!(progress.state.as_str(), "stalled" | "failed")
            || !progress.critical_services_healthy
        {
            return ProgressState::Failed;
        }
        if !progress.status_ok && progress.state != "starting" {
            return ProgressState::Failed;
        }
        self.last_progress_at_ms = progress.last_progress_at_ms;
        match self.last_sequence {
            Some(previous) if progress.reconcile_sequence > previous => {
                self.last_sequence = Some(progress.reconcile_sequence);
                self.unchanged_since_ms = timestamp_ms;
                ProgressState::Advanced
            }
            None => {
                self.last_sequence = Some(progress.reconcile_sequence);
                self.unchanged_since_ms = timestamp_ms;
                ProgressState::Waiting
            }
            Some(_) => self.no_progress(timestamp_ms, stall_timeout),
        }
    }

    fn no_progress(&mut self, timestamp_ms: u64, stall_timeout: Duration) -> ProgressState {
        if self.unchanged_since_ms == 0 {
            self.unchanged_since_ms = timestamp_ms;
            return ProgressState::Waiting;
        }
        let timeout_ms = u64::try_from(stall_timeout.as_millis()).unwrap_or(u64::MAX);
        if timestamp_ms.saturating_sub(self.unchanged_since_ms) > timeout_ms {
            ProgressState::Stalled
        } else {
            ProgressState::Waiting
        }
    }
}
