// =============================================================================
//        #######
//     ###       ###     F: outbox_journal.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Incremental binary journal for the durable synchronization outbox.

use crate::sync::error::{SyncError, SyncResult};
use crate::sync::outbox::SyncOutbox;
use crate::sync::outbox_format::{
    append_record, corrupt_record, create_empty_journal, encoded_frame_bytes,
    ensure_append_capacity, new_generation, outbox_full, read_header, record_hash, scan_records,
    validate_batch_id, validate_record_data, write_frame, write_header, JournalOperation,
    ScanResult, ACK_KIND, ACK_SPACE_RESERVE_BYTES, COMPACTION_ACK_RECORDS,
    COMPACTION_RECLAIM_BYTES, ENQUEUE_KIND, GENERATION_BYTES, HASH_BYTES, HEADER_BYTES,
    MAX_OUTBOX_FILE_BYTES,
};
use crate::sync::persistence::{acquire_persistence_lock, atomic_write_with, truncate_synced};
use crate::sync::types::SyncMessage;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

/// Stable on-disk format marker for incremental durable sync outboxes.
pub use crate::sync::outbox_format::SYNC_OUTBOX_FORMAT_V2;

/// Crash-consistent incremental file-backed synchronization outbox.
pub struct FileSyncOutbox {
    file_path: PathBuf,
    state: Mutex<JournalState>,
}

struct JournalState {
    generation: [u8; GENERATION_BYTES],
    messages: VecDeque<PendingMessage>,
    scanned_bytes: u64,
    record_count: u64,
    acknowledged_records: u64,
    live_frame_bytes: u64,
    chain_head: [u8; HASH_BYTES],
}

struct PendingMessage {
    message: SyncMessage,
    frame_bytes: u64,
}

impl std::fmt::Debug for FileSyncOutbox {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileSyncOutbox")
            .field("file_path", &self.file_path)
            .field("pending_messages", &self.state.lock().messages.len())
            .finish()
    }
}

impl FileSyncOutbox {
    /// Opens or creates a V2 outbox and validates every complete journal frame.
    pub fn new(file_path: impl Into<PathBuf>) -> SyncResult<Self> {
        let file_path = file_path.into();
        if let Some(parent) = file_path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
        }
        let _process_lock = acquire_persistence_lock(&file_path)?;
        if !file_path.exists() {
            create_empty_journal(&file_path)?;
        }
        let state = load_state(&file_path)?;
        Ok(Self {
            file_path,
            state: Mutex::new(state),
        })
    }

    /// Returns the durable outbox journal path.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    fn refresh(&self, state: &mut JournalState) -> SyncResult<()> {
        let header = read_header(&self.file_path)?;
        if header.generation != state.generation || header.file_bytes < state.scanned_bytes {
            *state = load_state(&self.file_path)?;
            return Ok(());
        }
        if header.file_bytes == state.scanned_bytes {
            return Ok(());
        }
        let batch_ids = state
            .messages
            .iter()
            .map(|pending| pending.message.batch_id.clone())
            .collect();
        let scan = scan_records(
            &self.file_path,
            state.scanned_bytes,
            state.record_count,
            state.chain_head,
            batch_ids,
        )?;
        apply_scan(state, scan, &self.file_path)
    }

    fn compact_if_needed(&self, state: &mut JournalState) -> SyncResult<()> {
        let reclaimable = state
            .scanned_bytes
            .saturating_sub(HEADER_BYTES as u64)
            .saturating_sub(state.live_frame_bytes);
        if reclaimable >= COMPACTION_RECLAIM_BYTES
            || state.acknowledged_records >= COMPACTION_ACK_RECORDS
        {
            compact(&self.file_path, state)?;
        }
        Ok(())
    }
}

impl SyncOutbox for FileSyncOutbox {
    fn try_enqueue(&self, message: SyncMessage, max_len: usize) -> SyncResult<bool> {
        validate_batch_id(&message.batch_id)?;
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        self.compact_if_needed(&mut state)?;
        if state.messages.len() >= max_len {
            return Ok(false);
        }
        let data = serde_json::to_vec(&message)
            .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
        validate_record_data(&data)?;
        let frame_bytes = encoded_frame_bytes(data.len())?;
        if state
            .scanned_bytes
            .checked_add(frame_bytes)
            .is_none_or(|size| size > MAX_OUTBOX_FILE_BYTES.saturating_sub(ACK_SPACE_RESERVE_BYTES))
        {
            compact(&self.file_path, &mut state)?;
        }
        ensure_append_capacity(state.scanned_bytes, frame_bytes, ACK_SPACE_RESERVE_BYTES)?;
        let hash = append_record(
            &self.file_path,
            state.record_count,
            state.chain_head,
            ENQUEUE_KIND,
            &data,
        )?;
        state.scanned_bytes += frame_bytes;
        state.record_count += 1;
        state.live_frame_bytes += frame_bytes;
        state.chain_head = hash;
        state.messages.push_back(PendingMessage {
            message,
            frame_bytes,
        });
        Ok(true)
    }

    fn front(&self) -> SyncResult<Option<SyncMessage>> {
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        Ok(state
            .messages
            .front()
            .map(|pending| pending.message.clone()))
    }

    fn acknowledge_front(&self, batch_id: &str) -> SyncResult<()> {
        validate_batch_id(batch_id)?;
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        self.compact_if_needed(&mut state)?;
        if state
            .messages
            .front()
            .map(|pending| pending.message.batch_id.as_str())
            != Some(batch_id)
        {
            return Err(SyncError::InvalidSyncMessage(
                "outbox acknowledgement mismatch",
            ));
        }
        let frame_bytes = encoded_frame_bytes(batch_id.len())?;
        ensure_append_capacity(state.scanned_bytes, frame_bytes, 0)?;
        let hash = append_record(
            &self.file_path,
            state.record_count,
            state.chain_head,
            ACK_KIND,
            batch_id.as_bytes(),
        )?;
        let acknowledged = state
            .messages
            .pop_front()
            .ok_or(SyncError::InvalidSyncMessage(
                "outbox acknowledgement mismatch",
            ))?;
        state.scanned_bytes += frame_bytes;
        state.record_count += 1;
        state.acknowledged_records += 1;
        state.live_frame_bytes = state
            .live_frame_bytes
            .saturating_sub(acknowledged.frame_bytes);
        state.chain_head = hash;
        Ok(())
    }

    fn messages(&self) -> SyncResult<Vec<SyncMessage>> {
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        Ok(state
            .messages
            .iter()
            .map(|pending| pending.message.clone())
            .collect())
    }

    fn len(&self) -> SyncResult<usize> {
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        Ok(state.messages.len())
    }
}

fn load_state(path: &Path) -> SyncResult<JournalState> {
    let header = read_header(path)?;
    let scan = scan_records(
        path,
        HEADER_BYTES as u64,
        0,
        [0; HASH_BYTES],
        VecDeque::new(),
    )?;
    let mut state = JournalState {
        generation: header.generation,
        messages: VecDeque::new(),
        scanned_bytes: HEADER_BYTES as u64,
        record_count: 0,
        acknowledged_records: 0,
        live_frame_bytes: 0,
        chain_head: [0; HASH_BYTES],
    };
    apply_scan(&mut state, scan, path)?;
    Ok(state)
}

fn apply_scan(state: &mut JournalState, scan: ScanResult, path: &Path) -> SyncResult<()> {
    if scan.recovered_tail {
        truncate_synced(path, scan.scanned_bytes)?;
    }
    for operation in scan.operations {
        match operation {
            JournalOperation::Enqueue {
                message,
                frame_bytes,
            } => {
                state.live_frame_bytes += frame_bytes;
                state.messages.push_back(PendingMessage {
                    message,
                    frame_bytes,
                });
            }
            JournalOperation::Acknowledge => {
                let acknowledged = state
                    .messages
                    .pop_front()
                    .ok_or_else(|| corrupt_record(state.record_count))?;
                state.live_frame_bytes = state
                    .live_frame_bytes
                    .saturating_sub(acknowledged.frame_bytes);
                state.acknowledged_records += 1;
            }
        }
    }
    state.scanned_bytes = scan.scanned_bytes;
    state.record_count = scan.record_count;
    state.chain_head = scan.chain_head;
    Ok(())
}

fn compact(path: &Path, state: &mut JournalState) -> SyncResult<()> {
    let generation = new_generation();
    let mut frame_sizes = Vec::with_capacity(state.messages.len());
    let mut chain_head = [0; HASH_BYTES];
    let mut record_count = 0u64;
    let mut total_bytes = HEADER_BYTES as u64;
    atomic_write_with(path, |file| {
        write_header(file, generation)?;
        for pending in &state.messages {
            let data = serde_json::to_vec(&pending.message)
                .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
            validate_record_data(&data)?;
            record_count += 1;
            let hash = record_hash(record_count, ENQUEUE_KIND, &data, chain_head);
            write_frame(file, record_count, ENQUEUE_KIND, &data, chain_head, hash)?;
            chain_head = hash;
            let size = encoded_frame_bytes(data.len())?;
            total_bytes = total_bytes.checked_add(size).ok_or_else(outbox_full)?;
            if total_bytes > MAX_OUTBOX_FILE_BYTES {
                return Err(outbox_full());
            }
            frame_sizes.push(size);
        }
        Ok(())
    })?;
    for (pending, frame_bytes) in state.messages.iter_mut().zip(frame_sizes) {
        pending.frame_bytes = frame_bytes;
    }
    state.generation = generation;
    state.scanned_bytes = total_bytes;
    state.record_count = record_count;
    state.acknowledged_records = 0;
    state.live_frame_bytes = total_bytes.saturating_sub(HEADER_BYTES as u64);
    state.chain_head = chain_head;
    Ok(())
}
