// =============================================================================
//        #######
//     ###       ###     F: limits.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Validated count, byte and monotonic-time ceilings for one UI surface.
use crate::{UiError, UiResult};
/// Budgets selected explicitly by the deployment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UiLimits {
    /// Maximum complete encoded request/response bytes.
    pub max_frame_bytes: usize,
    /// Maximum encoded application payload bytes.
    pub max_payload_bytes: usize,
    /// Maximum admitted operations in this surface.
    pub max_inflight: usize,
    /// Maximum active subscriptions in this surface.
    pub max_subscriptions: usize,
    /// Maximum retained events per subscription.
    pub max_events_per_subscription: usize,
    /// Aggregate accounted bytes in subscription queues.
    pub max_queued_event_bytes: usize,
    /// Maximum execution budget supplied in one request.
    pub max_timeout_ms: u32,
}
impl Default for UiLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: 256 * 1024,
            max_payload_bytes: 64 * 1024,
            max_inflight: 16,
            max_subscriptions: 32,
            max_events_per_subscription: 64,
            max_queued_event_bytes: 16 * 1024 * 1024,
            max_timeout_ms: 30_000,
        }
    }
}
impl UiLimits {
    /// Rejects zero, excessive and incoherent budgets before surface creation.
    pub fn validate(self) -> UiResult<Self> {
        if self.max_frame_bytes == 0
            || self.max_frame_bytes > 1024 * 1024
            || self.max_payload_bytes == 0
            || self.max_payload_bytes > self.max_frame_bytes
            || self.max_inflight == 0
            || self.max_inflight > 256
            || self.max_subscriptions == 0
            || self.max_subscriptions > 256
            || self.max_events_per_subscription == 0
            || self.max_events_per_subscription > 1024
            || self.max_queued_event_bytes == 0
            || self.max_queued_event_bytes > 64 * 1024 * 1024
            || self.max_timeout_ms == 0
            || self.max_timeout_ms > 60_000
        {
            return Err(UiError::InvalidRequest);
        }
        Ok(self)
    }
}
