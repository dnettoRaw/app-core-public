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
use crate::sync::outbox::{validate_page_limits, SyncOutbox, SyncOutboxReceipt, SyncOutboxStats};
use crate::sync::outbox_format::{
    append_record, corrupt_record, create_empty_journal, encode_attempt, encoded_frame_bytes,
    ensure_append_capacity, new_generation, outbox_full, read_header, record_hash, scan_records,
    validate_batch_id, validate_record_data, write_frame, write_header, JournalOperation,
    ScanPending, ScanResult, ACK_SPACE_RESERVE_BYTES, ATTEMPT_KIND, COMPACTION_ACK_RECORDS,
    COMPACTION_RECLAIM_BYTES, ENQUEUE_KIND, GENERATION_BYTES, HASH_BYTES, HEADER_BYTES,
    MAX_OUTBOX_FILE_BYTES, RECEIPT_KIND,
};
use crate::sync::outbox_journal_view::{page, stats, validate_receipt_prefix};
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

pub(super) struct PendingMessage {
    pub(super) message: SyncMessage,
    pub(super) encoded_bytes: usize,
    pub(super) frame_bytes: u64,
    pub(super) attempt_frame_bytes: u64,
    pub(super) attempts: u32,
    pub(super) next_ready_at_ms: u64,
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
        let pending = state
            .messages
            .iter()
            .map(|pending| ScanPending::new(pending.message.batch_id.clone(), pending.attempts))
            .collect();
        let scan = scan_records(
            &self.file_path,
            state.scanned_bytes,
            state.record_count,
            state.chain_head,
            pending,
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
            encoded_bytes: data.len(),
            frame_bytes,
            attempt_frame_bytes: 0,
            attempts: 0,
            next_ready_at_ms: 0,
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
        let receipt = SyncOutboxReceipt::new(vec![batch_id.to_string()])?;
        self.acknowledge_receipt(&receipt).map(|_| ())
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

    fn peek(&self, limit: usize, max_bytes: usize) -> SyncResult<Vec<SyncMessage>> {
        validate_page_limits(limit, max_bytes)?;
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        Ok(page(&state.messages, limit, max_bytes, None))
    }

    fn stats(&self) -> SyncResult<SyncOutboxStats> {
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        stats(&state.messages)
    }

    fn mark_attempt(&self, batch_id: &str, next_ready_at_ms: u64) -> SyncResult<u32> {
        validate_batch_id(batch_id)?;
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        self.compact_if_needed(&mut state)?;
        let pending = state
            .messages
            .front()
            .filter(|pending| pending.message.batch_id == batch_id)
            .ok_or(SyncError::InvalidSyncMessage("outbox attempt mismatch"))?;
        let attempts = pending
            .attempts
            .checked_add(1)
            .ok_or(SyncError::InvalidSyncMessage("outbox attempt overflow"))?;
        let previous_attempt_bytes = pending.attempt_frame_bytes;
        let data = encode_attempt(batch_id, attempts, next_ready_at_ms)?;
        let frame_bytes = encoded_frame_bytes(data.len())?;
        ensure_append_capacity(state.scanned_bytes, frame_bytes, ACK_SPACE_RESERVE_BYTES)?;
        let hash = append_record(
            &self.file_path,
            state.record_count,
            state.chain_head,
            ATTEMPT_KIND,
            &data,
        )?;
        let pending = state
            .messages
            .front_mut()
            .ok_or(SyncError::InvalidSyncMessage("outbox attempt mismatch"))?;
        pending.attempts = attempts;
        pending.next_ready_at_ms = next_ready_at_ms;
        pending.attempt_frame_bytes = frame_bytes;
        state.scanned_bytes += frame_bytes;
        state.record_count += 1;
        state.live_frame_bytes = state
            .live_frame_bytes
            .saturating_sub(previous_attempt_bytes)
            .saturating_add(frame_bytes);
        state.chain_head = hash;
        Ok(attempts)
    }

    fn next_ready(
        &self,
        now_ms: u64,
        limit: usize,
        max_bytes: usize,
    ) -> SyncResult<Vec<SyncMessage>> {
        validate_page_limits(limit, max_bytes)?;
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        Ok(page(&state.messages, limit, max_bytes, Some(now_ms)))
    }

    fn acknowledge_receipt(&self, receipt: &SyncOutboxReceipt) -> SyncResult<usize> {
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        self.compact_if_needed(&mut state)?;
        validate_receipt_prefix(&state.messages, receipt)?;
        let data = serde_json::to_vec(receipt.batch_ids())
            .map_err(|_| SyncError::InvalidSyncMessage("outbox receipt serialization"))?;
        validate_record_data(&data)?;
        let frame_bytes = encoded_frame_bytes(data.len())?;
        ensure_append_capacity(state.scanned_bytes, frame_bytes, 0)?;
        let hash = append_record(
            &self.file_path,
            state.record_count,
            state.chain_head,
            RECEIPT_KIND,
            &data,
        )?;
        let mut removed_live_bytes = 0u64;
        for _ in receipt.batch_ids() {
            let pending = state
                .messages
                .pop_front()
                .ok_or(SyncError::InvalidSyncMessage(
                    "outbox acknowledgement mismatch",
                ))?;
            removed_live_bytes = removed_live_bytes
                .saturating_add(pending.frame_bytes)
                .saturating_add(pending.attempt_frame_bytes);
        }
        state.scanned_bytes += frame_bytes;
        state.record_count += 1;
        state.acknowledged_records = state
            .acknowledged_records
            .saturating_add(receipt.batch_ids().len() as u64);
        state.live_frame_bytes = state.live_frame_bytes.saturating_sub(removed_live_bytes);
        state.chain_head = hash;
        Ok(receipt.batch_ids().len())
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
                encoded_bytes,
                frame_bytes,
            } => {
                state.live_frame_bytes += frame_bytes;
                state.messages.push_back(PendingMessage {
                    message,
                    encoded_bytes,
                    frame_bytes,
                    attempt_frame_bytes: 0,
                    attempts: 0,
                    next_ready_at_ms: 0,
                });
            }
            JournalOperation::Acknowledge { count } => {
                for _ in 0..count {
                    let acknowledged = state
                        .messages
                        .pop_front()
                        .ok_or_else(|| corrupt_record(state.record_count))?;
                    state.live_frame_bytes = state
                        .live_frame_bytes
                        .saturating_sub(acknowledged.frame_bytes)
                        .saturating_sub(acknowledged.attempt_frame_bytes);
                }
                state.acknowledged_records =
                    state.acknowledged_records.saturating_add(count as u64);
            }
            JournalOperation::Attempt {
                attempts,
                next_ready_at_ms,
                frame_bytes,
            } => {
                let pending = state
                    .messages
                    .front_mut()
                    .ok_or_else(|| corrupt_record(state.record_count))?;
                state.live_frame_bytes = state
                    .live_frame_bytes
                    .saturating_sub(pending.attempt_frame_bytes)
                    .saturating_add(frame_bytes);
                pending.attempt_frame_bytes = frame_bytes;
                pending.attempts = attempts;
                pending.next_ready_at_ms = next_ready_at_ms;
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
            let mut attempt_size = 0;
            if pending.attempts > 0 {
                let attempt = encode_attempt(
                    &pending.message.batch_id,
                    pending.attempts,
                    pending.next_ready_at_ms,
                )?;
                record_count += 1;
                let hash = record_hash(record_count, ATTEMPT_KIND, &attempt, chain_head);
                write_frame(file, record_count, ATTEMPT_KIND, &attempt, chain_head, hash)?;
                chain_head = hash;
                attempt_size = encoded_frame_bytes(attempt.len())?;
                total_bytes = total_bytes
                    .checked_add(attempt_size)
                    .ok_or_else(outbox_full)?;
                if total_bytes > MAX_OUTBOX_FILE_BYTES {
                    return Err(outbox_full());
                }
            }
            frame_sizes.push((data.len(), size, attempt_size));
        }
        Ok(())
    })?;
    for (pending, (encoded_bytes, frame_bytes, attempt_frame_bytes)) in
        state.messages.iter_mut().zip(frame_sizes)
    {
        pending.encoded_bytes = encoded_bytes;
        pending.frame_bytes = frame_bytes;
        pending.attempt_frame_bytes = attempt_frame_bytes;
    }
    state.generation = generation;
    state.scanned_bytes = total_bytes;
    state.record_count = record_count;
    state.acknowledged_records = 0;
    state.live_frame_bytes = total_bytes.saturating_sub(HEADER_BYTES as u64);
    state.chain_head = chain_head;
    Ok(())
}
