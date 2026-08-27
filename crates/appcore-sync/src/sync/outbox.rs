// =============================================================================
//        #######
//     ###       ###     F: outbox.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 10:48:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Durable bounded outbox contracts and process-local implementation.

use crate::sync::error::{SyncError, SyncResult};
use crate::sync::types::SyncMessage;
use parking_lot::Mutex;
use std::collections::VecDeque;

pub use crate::sync::outbox_journal::{FileSyncOutbox, SYNC_OUTBOX_FORMAT_V2};

/// Maximum number of messages returned by one bounded outbox read.
pub const MAX_OUTBOX_PAGE_MESSAGES: usize = 1_024;
/// Maximum encoded message bytes returned by one bounded outbox read.
pub const MAX_OUTBOX_PAGE_BYTES: usize = 48 * 1024 * 1024;

/// Bounded acknowledgement for an ordered prefix of the outbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncOutboxReceipt {
    batch_ids: Vec<String>,
}

impl SyncOutboxReceipt {
    /// Builds a non-empty bounded receipt in delivery order.
    pub fn new(batch_ids: Vec<String>) -> SyncResult<Self> {
        if batch_ids.is_empty() || batch_ids.len() > MAX_OUTBOX_PAGE_MESSAGES {
            return Err(SyncError::InvalidSyncMessage("invalid outbox receipt"));
        }
        for batch_id in &batch_ids {
            validate_outbox_batch_id(batch_id)?;
        }
        Ok(Self { batch_ids })
    }

    /// Returns acknowledged batch identifiers in delivery order.
    pub fn batch_ids(&self) -> &[String] {
        &self.batch_ids
    }
}

/// Bounded outbox observations without message payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncOutboxStats {
    /// Number of pending messages.
    pub pending_messages: usize,
    /// Total encoded pending bytes when the provider can report them exactly.
    pub pending_bytes: Option<usize>,
    /// Number of pending messages that have at least one delivery attempt.
    pub attempted_messages: Option<usize>,
    /// Total attempts across pending messages when known.
    pub total_attempts: Option<u64>,
    /// Readiness timestamp of the ordered front message when known.
    pub next_ready_at_ms: Option<u64>,
}

/// Ordered bounded queue that retains replication batches until acknowledgement.
pub trait SyncOutbox: Send + Sync {
    /// Enqueues a batch if the current length is below `max_len`.
    fn try_enqueue(&self, message: SyncMessage, max_len: usize) -> SyncResult<bool>;
    /// Returns the oldest pending batch.
    fn front(&self) -> SyncResult<Option<SyncMessage>>;
    /// Removes the oldest batch only when its identifier matches `batch_id`.
    fn acknowledge_front(&self, batch_id: &str) -> SyncResult<()>;
    /// Returns all pending batches in delivery order.
    fn messages(&self) -> SyncResult<Vec<SyncMessage>>;
    /// Returns the number of pending batches.
    fn len(&self) -> SyncResult<usize>;
    /// Returns a delivery-order page bounded before cloning message payloads.
    ///
    /// The compatibility default returns at most the front message. Providers
    /// should override this method to offer real multi-message pagination.
    fn peek(&self, limit: usize, max_bytes: usize) -> SyncResult<Vec<SyncMessage>> {
        validate_page_limits(limit, max_bytes)?;
        let Some(message) = self.front()? else {
            return Ok(Vec::new());
        };
        if limit == 0 || max_bytes == 0 || encoded_message_bytes(&message)? > max_bytes {
            return Ok(Vec::new());
        }
        Ok(vec![message])
    }
    /// Returns payload-free queue statistics.
    ///
    /// The compatibility default reports only the exact pending count and
    /// leaves unavailable observations from pre-extension providers as `None`.
    fn stats(&self) -> SyncResult<SyncOutboxStats> {
        Ok(SyncOutboxStats {
            pending_messages: self.len()?,
            pending_bytes: None,
            attempted_messages: None,
            total_attempts: None,
            next_ready_at_ms: None,
        })
    }
    /// Records one failed delivery attempt and its next eligible timestamp.
    ///
    /// Providers without this extension fail explicitly instead of pretending
    /// to persist retry state.
    fn mark_attempt(&self, _batch_id: &str, _next_ready_at_ms: u64) -> SyncResult<u32> {
        Err(SyncError::OutboxOperationUnsupported("mark_attempt"))
    }
    /// Returns the ready delivery-order prefix within both page bounds.
    ///
    /// The compatibility default preserves pre-extension immediate readiness
    /// and returns at most one message.
    fn next_ready(
        &self,
        _now_ms: u64,
        limit: usize,
        max_bytes: usize,
    ) -> SyncResult<Vec<SyncMessage>> {
        validate_page_limits(limit, max_bytes)?;
        self.peek(limit.min(1), max_bytes)
    }
    /// Acknowledges the exact ordered prefix named by a partial receipt.
    ///
    /// The compatibility default accepts one identifier only. Providers should
    /// override this method to apply a multi-message receipt atomically.
    fn acknowledge_receipt(&self, receipt: &SyncOutboxReceipt) -> SyncResult<usize> {
        let [batch_id] = receipt.batch_ids() else {
            return Err(SyncError::OutboxOperationUnsupported(
                "multi-message receipt",
            ));
        };
        self.acknowledge_front(batch_id)?;
        Ok(1)
    }
    /// Reports whether no batches are pending.
    fn is_empty(&self) -> SyncResult<bool> {
        Ok(self.len()? == 0)
    }
}

#[derive(Debug, Default)]
/// Process-local synchronization outbox.
pub struct InMemorySyncOutbox {
    messages: Mutex<VecDeque<PendingMessage>>,
}

#[derive(Debug)]
struct PendingMessage {
    message: SyncMessage,
    encoded_bytes: usize,
    attempts: u32,
    next_ready_at_ms: u64,
}

impl InMemorySyncOutbox {
    /// Creates an empty in-memory outbox.
    pub fn new() -> Self {
        Self::default()
    }
}

impl SyncOutbox for InMemorySyncOutbox {
    fn try_enqueue(&self, message: SyncMessage, max_len: usize) -> SyncResult<bool> {
        let encoded_bytes = encoded_message_bytes(&message)?;
        let mut messages = self.messages.lock();
        if messages.len() >= max_len {
            return Ok(false);
        }
        messages.push_back(PendingMessage {
            message,
            encoded_bytes,
            attempts: 0,
            next_ready_at_ms: 0,
        });
        Ok(true)
    }

    fn front(&self) -> SyncResult<Option<SyncMessage>> {
        Ok(self
            .messages
            .lock()
            .front()
            .map(|pending| pending.message.clone()))
    }

    fn acknowledge_front(&self, batch_id: &str) -> SyncResult<()> {
        let mut messages = self.messages.lock();
        if messages
            .front()
            .map(|pending| pending.message.batch_id.as_str())
            != Some(batch_id)
        {
            return Err(SyncError::InvalidSyncMessage(
                "outbox acknowledgement mismatch",
            ));
        }
        messages.pop_front();
        Ok(())
    }

    fn messages(&self) -> SyncResult<Vec<SyncMessage>> {
        Ok(self
            .messages
            .lock()
            .iter()
            .map(|pending| pending.message.clone())
            .collect())
    }

    fn len(&self) -> SyncResult<usize> {
        Ok(self.messages.lock().len())
    }

    fn peek(&self, limit: usize, max_bytes: usize) -> SyncResult<Vec<SyncMessage>> {
        validate_page_limits(limit, max_bytes)?;
        Ok(page(self.messages.lock().iter(), limit, max_bytes, None))
    }

    fn stats(&self) -> SyncResult<SyncOutboxStats> {
        let messages = self.messages.lock();
        let pending_bytes = messages.iter().try_fold(0usize, |total, pending| {
            total
                .checked_add(pending.encoded_bytes)
                .ok_or(SyncError::InvalidSyncMessage("outbox byte overflow"))
        })?;
        let attempted_messages = messages
            .iter()
            .filter(|pending| pending.attempts > 0)
            .count();
        let total_attempts = messages.iter().try_fold(0u64, |total, pending| {
            total
                .checked_add(u64::from(pending.attempts))
                .ok_or(SyncError::InvalidSyncMessage("outbox attempt overflow"))
        })?;
        Ok(SyncOutboxStats {
            pending_messages: messages.len(),
            pending_bytes: Some(pending_bytes),
            attempted_messages: Some(attempted_messages),
            total_attempts: Some(total_attempts),
            next_ready_at_ms: messages.front().map(|pending| pending.next_ready_at_ms),
        })
    }

    fn mark_attempt(&self, batch_id: &str, next_ready_at_ms: u64) -> SyncResult<u32> {
        validate_outbox_batch_id(batch_id)?;
        let mut messages = self.messages.lock();
        let pending = messages
            .front_mut()
            .filter(|pending| pending.message.batch_id == batch_id)
            .ok_or(SyncError::InvalidSyncMessage("outbox attempt mismatch"))?;
        pending.attempts = pending
            .attempts
            .checked_add(1)
            .ok_or(SyncError::InvalidSyncMessage("outbox attempt overflow"))?;
        pending.next_ready_at_ms = next_ready_at_ms;
        Ok(pending.attempts)
    }

    fn next_ready(
        &self,
        now_ms: u64,
        limit: usize,
        max_bytes: usize,
    ) -> SyncResult<Vec<SyncMessage>> {
        validate_page_limits(limit, max_bytes)?;
        Ok(page(
            self.messages.lock().iter(),
            limit,
            max_bytes,
            Some(now_ms),
        ))
    }

    fn acknowledge_receipt(&self, receipt: &SyncOutboxReceipt) -> SyncResult<usize> {
        let mut messages = self.messages.lock();
        validate_receipt_prefix(&messages, receipt)?;
        for _ in receipt.batch_ids() {
            messages.pop_front();
        }
        Ok(receipt.batch_ids().len())
    }
}

fn page<'a>(
    messages: impl Iterator<Item = &'a PendingMessage>,
    limit: usize,
    max_bytes: usize,
    ready_at_ms: Option<u64>,
) -> Vec<SyncMessage> {
    let mut page = Vec::new();
    let mut bytes = 0usize;
    for pending in messages.take(limit) {
        if ready_at_ms.is_some_and(|now| pending.next_ready_at_ms > now)
            || bytes
                .checked_add(pending.encoded_bytes)
                .is_none_or(|total| total > max_bytes)
        {
            break;
        }
        bytes += pending.encoded_bytes;
        page.push(pending.message.clone());
    }
    page
}

fn validate_receipt_prefix(
    messages: &VecDeque<PendingMessage>,
    receipt: &SyncOutboxReceipt,
) -> SyncResult<()> {
    if messages.len() < receipt.batch_ids().len()
        || messages
            .iter()
            .zip(receipt.batch_ids())
            .any(|(pending, batch_id)| pending.message.batch_id != *batch_id)
    {
        return Err(SyncError::InvalidSyncMessage(
            "outbox acknowledgement mismatch",
        ));
    }
    Ok(())
}

pub(crate) fn encoded_message_bytes(message: &SyncMessage) -> SyncResult<usize> {
    serde_json::to_vec(message)
        .map(|encoded| encoded.len())
        .map_err(|_| SyncError::InvalidSyncMessage("outbox serialization failed"))
}

pub(crate) fn validate_page_limits(limit: usize, max_bytes: usize) -> SyncResult<()> {
    if limit > MAX_OUTBOX_PAGE_MESSAGES || max_bytes > MAX_OUTBOX_PAGE_BYTES {
        return Err(SyncError::InvalidSyncMessage("invalid outbox page limits"));
    }
    Ok(())
}

pub(crate) fn validate_outbox_batch_id(batch_id: &str) -> SyncResult<()> {
    if batch_id.is_empty() || batch_id.len() > 1_024 || batch_id.chars().any(char::is_control) {
        return Err(SyncError::InvalidSyncMessage("invalid outbox batch id"));
    }
    Ok(())
}
