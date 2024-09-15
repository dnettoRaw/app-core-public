// =============================================================================
//        #######
//     ###       ###     F: outbox.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 10:48:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Durable bounded outbox contracts for follower replication pushes.

use crate::sync::codec::{bytes_to_hex, hex_to_bytes};
use crate::sync::error::{SyncError, SyncResult};
use crate::sync::persistence::{
    acquire_persistence_lock, atomic_write, read_bounded_text, split_format,
};
use crate::sync::types::SyncMessage;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Stable on-disk format marker for durable sync outboxes.
pub const SYNC_OUTBOX_FORMAT_V1: &str = "# appcore-sync-outbox-v1";
const MAX_OUTBOX_FILE_BYTES: u64 = 64 * 1024 * 1024;

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

#[derive(Debug)]
/// Crash-consistent file-backed synchronization outbox.
pub struct FileSyncOutbox {
    file_path: PathBuf,
    messages: Mutex<VecDeque<SyncMessage>>,
}

impl FileSyncOutbox {
    /// Opens or creates an outbox and validates all existing records.
    pub fn new(file_path: impl Into<PathBuf>) -> SyncResult<Self> {
        let file_path = file_path.into();
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
        }
        let _process_lock = acquire_persistence_lock(&file_path)?;
        let existed = file_path.exists();
        let messages = if existed {
            load_messages(&file_path)?
        } else {
            VecDeque::new()
        };
        let outbox = Self {
            file_path,
            messages: Mutex::new(messages),
        };
        if !existed {
            outbox.replace(&outbox.messages.lock())?;
        }
        Ok(outbox)
    }

    /// Returns the durable outbox file path.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    fn replace(&self, messages: &VecDeque<SyncMessage>) -> SyncResult<()> {
        let mut encoded_file = format!("{SYNC_OUTBOX_FORMAT_V1}\n");
        for message in messages {
            let encoded = serde_json::to_vec(message)
                .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
            encoded_file.push_str(&bytes_to_hex(&encoded));
            encoded_file.push('\n');
            if encoded_file.len() as u64 > MAX_OUTBOX_FILE_BYTES {
                return Err(SyncError::ReplicationFailed(
                    "sync outbox exceeds configured limit".to_string(),
                ));
            }
        }
        atomic_write(&self.file_path, encoded_file.as_bytes())
    }
}

impl SyncOutbox for FileSyncOutbox {
    fn try_enqueue(&self, message: SyncMessage, max_len: usize) -> SyncResult<bool> {
        let mut messages = self.messages.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        *messages = load_messages(&self.file_path)?;
        if messages.len() >= max_len {
            return Ok(false);
        }
        let mut updated = messages.clone();
        updated.push_back(message);
        self.replace(&updated)?;
        *messages = updated;
        Ok(true)
    }

    fn front(&self) -> SyncResult<Option<SyncMessage>> {
        let mut messages = self.messages.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        *messages = load_messages(&self.file_path)?;
        Ok(messages.front().cloned())
    }

    fn acknowledge_front(&self, batch_id: &str) -> SyncResult<()> {
        let mut messages = self.messages.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        *messages = load_messages(&self.file_path)?;
        if messages.front().map(|message| message.batch_id.as_str()) != Some(batch_id) {
            return Err(SyncError::InvalidSyncMessage(
                "outbox acknowledgement mismatch",
            ));
        }
        let mut updated = messages.clone();
        updated.pop_front();
        self.replace(&updated)?;
        *messages = updated;
        Ok(())
    }

    fn messages(&self) -> SyncResult<Vec<SyncMessage>> {
        let mut messages = self.messages.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        *messages = load_messages(&self.file_path)?;
        Ok(messages.iter().cloned().collect())
    }

    fn len(&self) -> SyncResult<usize> {
        let mut messages = self.messages.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        *messages = load_messages(&self.file_path)?;
        Ok(messages.len())
    }
}

fn load_messages(path: &Path) -> SyncResult<VecDeque<SyncMessage>> {
    let contents = read_bounded_text(path, MAX_OUTBOX_FILE_BYTES)?;
    let formatted = split_format(&contents, SYNC_OUTBOX_FORMAT_V1)?;
    let mut messages = VecDeque::new();
    for (line_number, line) in formatted.body.lines().enumerate() {
        if line.is_empty() {
            continue;
        }
        let bytes = hex_to_bytes(line).map_err(|_| SyncError::CorruptOutbox {
            line: line_number + 1,
        })?;
        if !bytes.starts_with(b"{") {
            return Err(SyncError::ReplicationFailed(
                crate::sync::error::UPDATE_REQUIRED_MESSAGE.to_string(),
            ));
        }
        let message = serde_json::from_slice(&bytes).map_err(|_| SyncError::CorruptOutbox {
            line: line_number + 1,
        })?;
        messages.push_back(message);
    }
    Ok(messages)
}
