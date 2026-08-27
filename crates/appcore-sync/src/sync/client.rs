// =============================================================================
//        #######
//     ###       ###     F: client.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Follower push client with bounded queue and retry behavior.

use crate::sync::error::{SyncError, SyncResult};
use crate::sync::outbox::{
    FileSyncOutbox, InMemorySyncOutbox, SyncOutbox, SyncOutboxReceipt, SyncOutboxStats,
    MAX_OUTBOX_PAGE_BYTES,
};
use crate::sync::retry::{SyncPushMetrics, SyncRetryPolicy};
use crate::sync::transport::HttpSyncTransport;
use crate::sync::types::SyncMessage;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Pushes leader events to a follower over transport.
#[derive(Clone)]
pub struct FollowerSyncClient {
    transport: HttpSyncTransport,
    retry_policy: SyncRetryPolicy,
    outbox: Arc<dyn SyncOutbox>,
    flush_lock: Arc<Mutex<()>>,
    metrics: Arc<Mutex<SyncPushMetrics>>,
}

impl std::fmt::Debug for FollowerSyncClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FollowerSyncClient")
            .field("retry_policy", &self.retry_policy)
            .field("pending_len", &self.pending_len())
            .field("metrics", &self.metrics())
            .finish()
    }
}

impl FollowerSyncClient {
    /// Creates a follower client with bounded in-memory buffering and default retries.
    pub fn new(transport: HttpSyncTransport) -> Self {
        Self {
            transport,
            retry_policy: SyncRetryPolicy::default(),
            outbox: Arc::new(InMemorySyncOutbox::new()),
            flush_lock: Arc::new(Mutex::new(())),
            metrics: Arc::new(Mutex::new(SyncPushMetrics::default())),
        }
    }

    /// Replaces retry, backoff, and queue limits.
    pub fn with_retry_policy(mut self, retry_policy: SyncRetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    /// Returns the configured retry policy.
    pub fn retry_policy(&self) -> SyncRetryPolicy {
        self.retry_policy
    }

    /// Replaces the pending-message outbox implementation.
    pub fn with_outbox(mut self, outbox: Arc<dyn SyncOutbox>) -> Self {
        self.outbox = outbox;
        self
    }

    /// Configures a durable file outbox at `file_path`.
    pub fn with_file_outbox(self, file_path: impl Into<PathBuf>) -> SyncResult<Self> {
        Ok(self.with_outbox(Arc::new(FileSyncOutbox::new(file_path)?)))
    }

    /// Returns a snapshot of cumulative push counters.
    pub fn metrics(&self) -> SyncPushMetrics {
        *self.metrics.lock()
    }

    /// Returns the pending count, or zero if the outbox cannot be read.
    pub fn pending_len(&self) -> usize {
        self.outbox.len().unwrap_or(0)
    }

    /// Returns a complete compatibility snapshot of pending batches.
    ///
    /// New consumers should use [`Self::pending_page`] and [`Self::outbox_stats`]
    /// so queue growth cannot determine one allocation.
    pub fn pending_messages(&self) -> SyncResult<Vec<SyncMessage>> {
        self.outbox.messages()
    }

    /// Returns one pending page bounded before payload clones.
    pub fn pending_page(&self, limit: usize, max_bytes: usize) -> SyncResult<Vec<SyncMessage>> {
        self.outbox.peek(limit, max_bytes)
    }

    /// Returns payload-free outbox observations.
    pub fn outbox_stats(&self) -> SyncResult<SyncOutboxStats> {
        self.outbox.stats()
    }

    /// Cancels active transport I/O and retry waits.
    pub fn cancel(&self) {
        self.transport.cancel();
    }

    /// Reports whether this client has been cancelled.
    pub fn is_cancelled(&self) -> bool {
        self.transport.is_cancelled()
    }

    /// Attempts delivery of all currently queued batches.
    pub fn flush_pending(&self) -> SyncResult<()> {
        self.flush_queue().map(|_| ())
    }

    /// Attempts queued delivery and returns the last batch durably acknowledged.
    pub fn flush_pending_with_progress(&self) -> SyncResult<Option<SyncMessage>> {
        self.flush_queue()
    }

    /// Enqueues a batch durably before attempting ordered delivery.
    pub fn push_events(&self, message: &SyncMessage) -> SyncResult<()> {
        if !self
            .outbox
            .try_enqueue(message.clone(), self.retry_policy.max_queue_len)?
        {
            let mut metrics = self.metrics.lock();
            metrics.push_dropped += 1;
            return Err(SyncError::TransportFailed("sync queue full".to_string()));
        }
        self.flush_queue().map(|_| ())
    }

    fn flush_queue(&self) -> SyncResult<Option<SyncMessage>> {
        let _flush_guard = self.flush_lock.lock();
        let mut last_acknowledged = None;
        loop {
            let now_ms = unix_time_ms();
            let message = match self
                .outbox
                .next_ready(now_ms, 1, MAX_OUTBOX_PAGE_BYTES)?
                .into_iter()
                .next()
            {
                Some(message) => message,
                None if self.outbox.is_empty()? => return Ok(last_acknowledged),
                None => {
                    return Err(SyncError::TransportFailed(
                        "sync push retry deferred".to_string(),
                    ));
                }
            };
            if let Err(error) = self.try_send_with_retry(&message) {
                let mut metrics = self.metrics.lock();
                metrics.push_failed += 1;
                return Err(error);
            }
            let receipt = SyncOutboxReceipt::new(vec![message.batch_id.clone()])?;
            self.outbox.acknowledge_receipt(&receipt)?;
            let mut metrics = self.metrics.lock();
            metrics.push_success += 1;
            last_acknowledged = Some(message);
        }
    }

    fn try_send_with_retry(&self, message: &SyncMessage) -> SyncResult<()> {
        let max_attempts = self.retry_policy.max_attempts.max(1);
        for attempt in 1..=max_attempts {
            if self.transport.is_cancelled() {
                return Err(SyncError::TransportFailed(
                    "sync push cancelled".to_string(),
                ));
            }
            let mut metrics = self.metrics.lock();
            metrics.push_attempt += 1;
            drop(metrics);
            if self.transport.post_sync_events(message).is_ok() {
                return Ok(());
            }
            let next_ready_at_ms = unix_time_ms().saturating_add(self.retry_policy.backoff_ms);
            match self
                .outbox
                .mark_attempt(&message.batch_id, next_ready_at_ms)
            {
                Ok(_) | Err(SyncError::OutboxOperationUnsupported(_)) => {}
                Err(error) => return Err(error),
            }
            if attempt < max_attempts
                && self.retry_policy.backoff_ms > 0
                && self
                    .transport
                    .cancellation_token()
                    .wait_timeout(Duration::from_millis(self.retry_policy.backoff_ms))
            {
                return Err(SyncError::TransportFailed(
                    "sync push cancelled".to_string(),
                ));
            }
        }
        Err(SyncError::TransportFailed(
            "sync push retry exhausted".to_string(),
        ))
    }
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}
