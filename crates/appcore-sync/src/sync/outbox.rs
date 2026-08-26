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
    /// Reports whether no batches are pending.
    fn is_empty(&self) -> SyncResult<bool> {
        Ok(self.len()? == 0)
    }
}

#[derive(Debug, Default)]
/// Process-local synchronization outbox.
pub struct InMemorySyncOutbox {
    messages: Mutex<VecDeque<SyncMessage>>,
}

impl InMemorySyncOutbox {
    /// Creates an empty in-memory outbox.
    pub fn new() -> Self {
        Self::default()
    }
}

impl SyncOutbox for InMemorySyncOutbox {
    fn try_enqueue(&self, message: SyncMessage, max_len: usize) -> SyncResult<bool> {
        let mut messages = self.messages.lock();
        if messages.len() >= max_len {
            return Ok(false);
        }
        messages.push_back(message);
        Ok(true)
    }

    fn front(&self) -> SyncResult<Option<SyncMessage>> {
        Ok(self.messages.lock().front().cloned())
    }

    fn acknowledge_front(&self, batch_id: &str) -> SyncResult<()> {
        let mut messages = self.messages.lock();
        if messages.front().map(|message| message.batch_id.as_str()) != Some(batch_id) {
            return Err(SyncError::InvalidSyncMessage(
                "outbox acknowledgement mismatch",
            ));
        }
        messages.pop_front();
        Ok(())
    }

    fn messages(&self) -> SyncResult<Vec<SyncMessage>> {
        Ok(self.messages.lock().iter().cloned().collect())
    }

    fn len(&self) -> SyncResult<usize> {
        Ok(self.messages.lock().len())
    }
}
