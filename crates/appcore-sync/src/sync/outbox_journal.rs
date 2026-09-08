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
    ensure_append_capacity, new_generation, outbox_full, read_header, record_data_offset,
    record_hash, scan_records, validate_batch_id, write_frame, write_header,
    ACK_SPACE_RESERVE_BYTES, ATTEMPT_KIND, COMPACTION_ACK_RECORDS, COMPACTION_RECLAIM_BYTES,
    ENQUEUE_KIND, GENERATION_BYTES, HASH_BYTES, HEADER_BYTES, MAX_OUTBOX_FILE_BYTES, RECEIPT_KIND,
};
use crate::sync::outbox_journal_state::{JournalOperation, ScanPending, ScanResult};
use crate::sync::outbox_journal_view::{
    load_message, page, stats, validate_receipt_prefix, PendingMessage,
};
use crate::sync::outbox_stream::{append_json_record, measure_json, write_json_frame};
use crate::sync::persistence::{acquire_persistence_lock, atomic_write_with, truncate_synced};
use crate::sync::types::SyncMessage;
use parking_lot::Mutex;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::Arc;

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
            .map(|pending| ScanPending::new(Arc::clone(&pending.batch_id), pending.attempts))
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
        let measurement = measure_json(&message)?;
        let frame_bytes = encoded_frame_bytes(measurement.bytes)?;
        if state
            .scanned_bytes
            .checked_add(frame_bytes)
            .is_none_or(|size| size > MAX_OUTBOX_FILE_BYTES.saturating_sub(ACK_SPACE_RESERVE_BYTES))
        {
            compact(&self.file_path, &mut state)?;
        }
        ensure_append_capacity(state.scanned_bytes, frame_bytes, ACK_SPACE_RESERVE_BYTES)?;
        let frame_start = state.scanned_bytes;
        let hash = append_json_record(
            &self.file_path,
            state.record_count.saturating_add(1),
            ENQUEUE_KIND,
            &message,
            &measurement,
            state.chain_head,
        )?;
        state.scanned_bytes += frame_bytes;
        state.record_count += 1;
        state.live_frame_bytes += frame_bytes;
        state.chain_head = hash;
        let ordinal = state.record_count;
        let data_offset = record_data_offset(frame_start)?;
        state.messages.push_back(PendingMessage {
            batch_id: Arc::from(message.batch_id),
            ordinal,
            data_offset,
            data_digest: measurement.digest,
            encoded_bytes: measurement.bytes,
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
        state
            .messages
            .front()
            .map(|pending| load_message(&self.file_path, pending))
            .transpose()
    }

    fn acknowledge_front(&self, batch_id: &str) -> SyncResult<()> {
        let receipt = SyncOutboxReceipt::new(vec![batch_id.to_string()])?;
        self.acknowledge_receipt(&receipt).map(|_| ())
    }

    fn messages(&self) -> SyncResult<Vec<SyncMessage>> {
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        state
            .messages
            .iter()
            .map(|pending| load_message(&self.file_path, pending))
            .collect()
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
        page(&self.file_path, &state.messages, limit, max_bytes, None)
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
            .filter(|pending| pending.batch_id.as_ref() == batch_id)
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
        page(
            &self.file_path,
            &state.messages,
            limit,
            max_bytes,
            Some(now_ms),
        )
    }

    fn acknowledge_receipt(&self, receipt: &SyncOutboxReceipt) -> SyncResult<usize> {
        let mut state = self.state.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh(&mut state)?;
        self.compact_if_needed(&mut state)?;
        validate_receipt_prefix(&state.messages, receipt)?;
        let batch_ids = receipt.batch_ids();
        let measurement = measure_json(&batch_ids)?;
        let frame_bytes = encoded_frame_bytes(measurement.bytes)?;
        ensure_append_capacity(state.scanned_bytes, frame_bytes, 0)?;
        let hash = append_json_record(
            &self.file_path,
            state.record_count.saturating_add(1),
            RECEIPT_KIND,
            &batch_ids,
            &measurement,
            state.chain_head,
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
                batch_id,
                ordinal,
                data_offset,
                data_digest,
                encoded_bytes,
                frame_bytes,
            } => {
                state.live_frame_bytes += frame_bytes;
                state.messages.push_back(PendingMessage {
                    batch_id,
                    ordinal,
                    data_offset,
                    data_digest,
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
            let message = load_message(path, pending)?;
            let measurement = measure_json(&message)?;
            record_count += 1;
            let ordinal = record_count;
            let data_offset = record_data_offset(total_bytes)?;
            let hash = write_json_frame(
                file,
                record_count,
                ENQUEUE_KIND,
                &message,
                measurement.bytes,
                chain_head,
            )?;
            chain_head = hash;
            let size = encoded_frame_bytes(measurement.bytes)?;
            total_bytes = total_bytes.checked_add(size).ok_or_else(outbox_full)?;
            if total_bytes > MAX_OUTBOX_FILE_BYTES {
                return Err(outbox_full());
            }
            let mut attempt_size = 0;
            if pending.attempts > 0 {
                let attempt = encode_attempt(
                    &pending.batch_id,
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
            frame_sizes.push((
                ordinal,
                data_offset,
                measurement.digest,
                measurement.bytes,
                size,
                attempt_size,
            ));
        }
        Ok(())
    })?;
    for (
        pending,
        (ordinal, data_offset, data_digest, encoded_bytes, frame_bytes, attempt_frame_bytes),
    ) in state.messages.iter_mut().zip(frame_sizes)
    {
        pending.ordinal = ordinal;
        pending.data_offset = data_offset;
        pending.data_digest = data_digest;
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
