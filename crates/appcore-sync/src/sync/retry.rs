// =============================================================================
//        #######
//     ###       ###     F: retry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Retry, queue, and metrics contracts for follower pushes.

/// Retry and queue limits for follower push.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncRetryPolicy {
    /// Maximum delivery attempts per flush, with zero treated as one.
    pub max_attempts: u32,
    /// Fixed delay between attempts in milliseconds.
    pub backoff_ms: u64,
    /// Maximum number of batches retained by the outbox.
    pub max_queue_len: usize,
}

impl Default for SyncRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_ms: 50,
            max_queue_len: 64,
        }
    }
}

/// Basic sync push metrics for local observability.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SyncPushMetrics {
    /// Total transport attempts.
    pub push_attempt: u64,
    /// Total batches acknowledged and removed from the outbox.
    pub push_success: u64,
    /// Total flushes that exhausted their retry policy.
    pub push_failed: u64,
    /// Total batches rejected because the outbox was full.
    pub push_dropped: u64,
}
