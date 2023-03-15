// =============================================================================
//        #######
//     ###       ###     F: events.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded, non-sensitive supervisor and watchdog events.

/// Stable kind emitted for supervisor and watchdog state changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisorEventKind {
    /// A service started.
    ServiceStarted,
    /// A service stopped.
    ServiceStopped,
    /// A restart was accepted into the bounded schedule.
    ServiceRestartScheduled,
    /// A service restarted.
    ServiceRestarted,
    /// A service failed.
    ServiceFailed,
    /// A degraded or failed service recovered.
    ServiceRecovered,
    /// A service instance outlived its shutdown timeout.
    ServiceOrphaned,
    /// A service exhausted its restart budget.
    RestartBudgetExceeded,
    /// A service requires operator intervention.
    ServiceQuarantined,
    /// A reconciliation cycle completed.
    SupervisorProgressed,
    /// The watchdog detected absent reconciliation progress.
    SupervisorStalled,
    /// Reconciliation resumed after a stall.
    SupervisorRecovered,
}

/// One bounded, non-sensitive supervisor event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisorEvent {
    /// Service identifier, or `supervisor` for process-level events.
    pub service_id: String,
    /// Stable event kind.
    pub kind: SupervisorEventKind,
    /// Event time in Unix milliseconds.
    pub timestamp_ms: u64,
    /// Restart attempt or reconcile sequence associated with the event.
    pub attempt: u64,
    /// Controlled reason code without service payloads or secrets.
    pub reason: String,
    /// Stable previous state label.
    pub previous_state: String,
    /// Stable new state label.
    pub new_state: String,
    /// Process-local trace identifier.
    pub trace_id: String,
}

impl SupervisorEvent {
    pub(crate) fn new(
        service_id: impl Into<String>,
        kind: SupervisorEventKind,
        timestamp_ms: u64,
        attempt: u64,
        transition: (&str, &str),
        reason: &str,
        trace_id: String,
    ) -> Self {
        Self {
            service_id: service_id.into(),
            kind,
            timestamp_ms,
            attempt,
            reason: reason.to_string(),
            previous_state: transition.0.to_string(),
            new_state: transition.1.to_string(),
            trace_id,
        }
    }
}
