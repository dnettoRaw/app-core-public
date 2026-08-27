// =============================================================================
//        #######
//     ###       ###     F: outbox_format.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Binary framing, integrity and tail scanning for outbox journal V2.

use crate::sync::error::{SyncError, SyncResult, UPDATE_REQUIRED_MESSAGE};
use crate::sync::persistence::{atomic_write_with, reject_symlink};
use crate::sync::types::SyncMessage;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable on-disk format marker for incremental durable sync outboxes.
pub const SYNC_OUTBOX_FORMAT_V2: &str = "appcore-sync-outbox-v2";
const MAGIC: &[u8] = b"appcore-sync-outbox-v2\0";
pub(super) const GENERATION_BYTES: usize = 16;
pub(super) const HASH_BYTES: usize = 32;
pub(super) const HEADER_BYTES: usize = MAGIC.len() + GENERATION_BYTES + HASH_BYTES;
const FRAME_BODY_FIXED_BYTES: usize = 8 + 1 + 4 + HASH_BYTES + HASH_BYTES + 4;
pub(super) const MAX_OUTBOX_FILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RECORD_DATA_BYTES: usize = 48 * 1024 * 1024;
pub(super) const ACK_SPACE_RESERVE_BYTES: u64 = 2 * 1024 * 1024;
const MAX_BATCH_ID_BYTES: usize = 1024;
pub(super) const COMPACTION_RECLAIM_BYTES: u64 = 4 * 1024 * 1024;
pub(super) const COMPACTION_ACK_RECORDS: u64 = 1024;
pub(super) const ENQUEUE_KIND: u8 = 1;
pub(super) const ACK_KIND: u8 = 2;
pub(super) const ATTEMPT_KIND: u8 = 3;
pub(super) const RECEIPT_KIND: u8 = 4;

static GENERATION_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) enum JournalOperation {
    Enqueue {
        message: SyncMessage,
        encoded_bytes: usize,
        frame_bytes: u64,
    },
    Acknowledge {
        count: usize,
    },
    Attempt {
        attempts: u32,
        next_ready_at_ms: u64,
        frame_bytes: u64,
    },
}

pub(super) struct ScanPending {
    batch_id: String,
    attempts: u32,
}

impl ScanPending {
    pub(super) fn new(batch_id: String, attempts: u32) -> Self {
        Self { batch_id, attempts }
    }
}

pub(super) struct ScanResult {
    pub(super) operations: Vec<JournalOperation>,
    pub(super) scanned_bytes: u64,
    pub(super) record_count: u64,
    pub(super) chain_head: [u8; HASH_BYTES],
    pub(super) recovered_tail: bool,
}

pub(super) struct JournalHeader {
    pub(super) generation: [u8; GENERATION_BYTES],
    pub(super) file_bytes: u64,
}

pub(super) fn create_empty_journal(path: &Path) -> SyncResult<()> {
    let generation = new_generation();
    atomic_write_with(path, |file| write_header(file, generation))
}

pub(super) fn read_header(path: &Path) -> SyncResult<JournalHeader> {
    reject_symlink(path)?;
    let mut file = File::open(path).map_err(io_error)?;
    let file_bytes = file.metadata().map_err(io_error)?.len();
    if file_bytes > MAX_OUTBOX_FILE_BYTES {
        return Err(outbox_full());
    }
    let mut header = Vec::with_capacity(HEADER_BYTES);
    Read::by_ref(&mut file)
        .take(HEADER_BYTES as u64)
        .read_to_end(&mut header)
        .map_err(io_error)?;
    if header.len() < MAGIC.len() || &header[..MAGIC.len()] != MAGIC {
        return Err(SyncError::ReplicationFailed(
            UPDATE_REQUIRED_MESSAGE.to_string(),
        ));
    }
    if header.len() < HEADER_BYTES {
        return Err(SyncError::CorruptOutbox { line: 1 });
    }
    let generation_start = MAGIC.len();
    let hash_start = generation_start + GENERATION_BYTES;
    let generation: [u8; GENERATION_BYTES] = header[generation_start..hash_start]
        .try_into()
        .map_err(|_| SyncError::CorruptOutbox { line: 1 })?;
    if header[hash_start..] != header_hash(generation) {
        return Err(SyncError::CorruptOutbox { line: 1 });
    }
    Ok(JournalHeader {
        generation,
        file_bytes,
    })
}

pub(super) fn scan_records(
    path: &Path,
    start: u64,
    record_count: u64,
    chain_head: [u8; HASH_BYTES],
    mut pending: VecDeque<ScanPending>,
) -> SyncResult<ScanResult> {
    let file = File::open(path).map_err(io_error)?;
    let file_bytes = file.metadata().map_err(io_error)?.len();
    let mut reader = BufReader::new(file);
    reader.seek(SeekFrom::Start(start)).map_err(io_error)?;
    let mut offset = start;
    let mut ordinal = record_count;
    let mut previous_hash = chain_head;
    let mut operations = Vec::new();
    let mut recovered_tail = false;
    while offset < file_bytes {
        let frame_start = offset;
        let Some(frame_len) = read_frame_length(&mut reader)? else {
            recovered_tail = true;
            offset = frame_start;
            break;
        };
        if !(FRAME_BODY_FIXED_BYTES..=FRAME_BODY_FIXED_BYTES + MAX_RECORD_DATA_BYTES)
            .contains(&frame_len)
        {
            return Err(corrupt_record(ordinal));
        }
        if frame_len as u64 > file_bytes.saturating_sub(frame_start).saturating_sub(4) {
            recovered_tail = true;
            offset = frame_start;
            break;
        }
        let mut frame = vec![0u8; frame_len];
        if !read_complete(&mut reader, &mut frame).map_err(io_error)? {
            recovered_tail = true;
            offset = frame_start;
            break;
        }
        ordinal += 1;
        let (operation, hash) = decode_frame(&frame, ordinal, previous_hash, &mut pending)?;
        previous_hash = hash;
        offset = frame_start + 4 + frame_len as u64;
        operations.push(operation);
    }
    Ok(ScanResult {
        operations,
        scanned_bytes: offset,
        record_count: ordinal,
        chain_head: previous_hash,
        recovered_tail,
    })
}

fn read_frame_length(reader: &mut impl Read) -> SyncResult<Option<usize>> {
    let mut bytes = [0u8; 4];
    let mut read = 0usize;
    while read < bytes.len() {
        match reader.read(&mut bytes[read..]).map_err(io_error)? {
            0 => return Ok(None),
            count => read += count,
        }
    }
    Ok(Some(u32::from_be_bytes(bytes) as usize))
}

fn read_complete(reader: &mut impl Read, bytes: &mut [u8]) -> std::io::Result<bool> {
    let mut read = 0usize;
    while read < bytes.len() {
        match reader.read(&mut bytes[read..])? {
            0 => return Ok(false),
            count => read += count,
        }
    }
    Ok(true)
}

fn decode_frame(
    frame: &[u8],
    expected_ordinal: u64,
    expected_previous: [u8; HASH_BYTES],
    pending: &mut VecDeque<ScanPending>,
) -> SyncResult<(JournalOperation, [u8; HASH_BYTES])> {
    let ordinal = u64::from_be_bytes(frame[0..8].try_into().map_err(|_| corrupt_record(0))?);
    if ordinal != expected_ordinal {
        return Err(corrupt_record(expected_ordinal.saturating_sub(1)));
    }
    let kind = frame[8];
    let data_len = u32::from_be_bytes(
        frame[9..13]
            .try_into()
            .map_err(|_| corrupt_record(ordinal))?,
    ) as usize;
    if data_len > MAX_RECORD_DATA_BYTES || frame.len() != FRAME_BODY_FIXED_BYTES + data_len {
        return Err(corrupt_record(ordinal));
    }
    let data_end = 13 + data_len;
    let previous_end = data_end + HASH_BYTES;
    let hash_end = previous_end + HASH_BYTES;
    let previous: [u8; HASH_BYTES] = frame[data_end..previous_end]
        .try_into()
        .map_err(|_| corrupt_record(ordinal))?;
    let hash: [u8; HASH_BYTES] = frame[previous_end..hash_end]
        .try_into()
        .map_err(|_| corrupt_record(ordinal))?;
    let trailing_len = u32::from_be_bytes(
        frame[hash_end..hash_end + 4]
            .try_into()
            .map_err(|_| corrupt_record(ordinal))?,
    ) as usize;
    if previous != expected_previous
        || trailing_len != frame.len()
        || hash != record_hash(ordinal, kind, &frame[13..data_end], previous)
    {
        return Err(corrupt_record(ordinal));
    }
    let frame_bytes = frame.len() as u64 + 4;
    let operation = match kind {
        ENQUEUE_KIND => decode_enqueue(&frame[13..data_end], frame_bytes, ordinal, pending)?,
        ACK_KIND => decode_ack(&frame[13..data_end], ordinal, pending)?,
        ATTEMPT_KIND => decode_attempt(&frame[13..data_end], frame_bytes, ordinal, pending)?,
        RECEIPT_KIND => decode_receipt(&frame[13..data_end], ordinal, pending)?,
        _ => return Err(corrupt_record(ordinal)),
    };
    Ok((operation, hash))
}

fn decode_enqueue(
    data: &[u8],
    frame_bytes: u64,
    ordinal: u64,
    pending: &mut VecDeque<ScanPending>,
) -> SyncResult<JournalOperation> {
    if !data.starts_with(b"{") {
        return Err(SyncError::ReplicationFailed(
            UPDATE_REQUIRED_MESSAGE.to_string(),
        ));
    }
    let message =
        serde_json::from_slice::<SyncMessage>(data).map_err(|_| corrupt_record(ordinal))?;
    validate_batch_id(&message.batch_id).map_err(|_| corrupt_record(ordinal))?;
    let encoded_bytes = data.len();
    pending.push_back(ScanPending::new(message.batch_id.clone(), 0));
    Ok(JournalOperation::Enqueue {
        message,
        encoded_bytes,
        frame_bytes,
    })
}

fn decode_ack(
    data: &[u8],
    ordinal: u64,
    pending: &mut VecDeque<ScanPending>,
) -> SyncResult<JournalOperation> {
    let batch_id = std::str::from_utf8(data).map_err(|_| corrupt_record(ordinal))?;
    validate_batch_id(batch_id).map_err(|_| corrupt_record(ordinal))?;
    if pending.front().map(|item| item.batch_id.as_str()) != Some(batch_id) {
        return Err(corrupt_record(ordinal));
    }
    pending.pop_front();
    Ok(JournalOperation::Acknowledge { count: 1 })
}

fn decode_attempt(
    data: &[u8],
    frame_bytes: u64,
    ordinal: u64,
    pending: &mut VecDeque<ScanPending>,
) -> SyncResult<JournalOperation> {
    if data.len() < 12 {
        return Err(corrupt_record(ordinal));
    }
    let attempts = u32::from_be_bytes(data[..4].try_into().map_err(|_| corrupt_record(ordinal))?);
    let next_ready_at_ms = u64::from_be_bytes(
        data[4..12]
            .try_into()
            .map_err(|_| corrupt_record(ordinal))?,
    );
    let batch_id = std::str::from_utf8(&data[12..]).map_err(|_| corrupt_record(ordinal))?;
    validate_batch_id(batch_id).map_err(|_| corrupt_record(ordinal))?;
    let Some(front) = pending.front_mut() else {
        return Err(corrupt_record(ordinal));
    };
    if front.batch_id != batch_id || front.attempts.checked_add(1) != Some(attempts) {
        return Err(corrupt_record(ordinal));
    }
    front.attempts = attempts;
    Ok(JournalOperation::Attempt {
        attempts,
        next_ready_at_ms,
        frame_bytes,
    })
}

fn decode_receipt(
    data: &[u8],
    ordinal: u64,
    pending: &mut VecDeque<ScanPending>,
) -> SyncResult<JournalOperation> {
    let batch_ids =
        serde_json::from_slice::<Vec<String>>(data).map_err(|_| corrupt_record(ordinal))?;
    if batch_ids.is_empty() || batch_ids.len() > crate::MAX_OUTBOX_PAGE_MESSAGES {
        return Err(corrupt_record(ordinal));
    }
    if pending.len() < batch_ids.len()
        || pending
            .iter()
            .zip(&batch_ids)
            .any(|(item, batch_id)| item.batch_id != *batch_id)
    {
        return Err(corrupt_record(ordinal));
    }
    for batch_id in &batch_ids {
        validate_batch_id(batch_id).map_err(|_| corrupt_record(ordinal))?;
        pending.pop_front();
    }
    Ok(JournalOperation::Acknowledge {
        count: batch_ids.len(),
    })
}

pub(super) fn encode_attempt(
    batch_id: &str,
    attempts: u32,
    next_ready_at_ms: u64,
) -> SyncResult<Vec<u8>> {
    validate_batch_id(batch_id)?;
    let mut data = Vec::with_capacity(12 + batch_id.len());
    data.extend_from_slice(&attempts.to_be_bytes());
    data.extend_from_slice(&next_ready_at_ms.to_be_bytes());
    data.extend_from_slice(batch_id.as_bytes());
    validate_record_data(&data)?;
    Ok(data)
}

pub(super) fn append_record(
    path: &Path,
    record_count: u64,
    chain_head: [u8; HASH_BYTES],
    kind: u8,
    data: &[u8],
) -> SyncResult<[u8; HASH_BYTES]> {
    reject_symlink(path)?;
    let ordinal = record_count.saturating_add(1);
    let hash = record_hash(ordinal, kind, data, chain_head);
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(io_error)?;
    write_frame(&mut file, ordinal, kind, data, chain_head, hash)?;
    file.sync_data().map_err(io_error)?;
    Ok(hash)
}

pub(super) fn write_header(file: &mut File, generation: [u8; GENERATION_BYTES]) -> SyncResult<()> {
    file.write_all(MAGIC)
        .and_then(|_| file.write_all(&generation))
        .and_then(|_| file.write_all(&header_hash(generation)))
        .map_err(io_error)
}

pub(super) fn write_frame(
    file: &mut File,
    ordinal: u64,
    kind: u8,
    data: &[u8],
    previous: [u8; HASH_BYTES],
    hash: [u8; HASH_BYTES],
) -> SyncResult<()> {
    let frame_len = FRAME_BODY_FIXED_BYTES
        .checked_add(data.len())
        .and_then(|length| u32::try_from(length).ok())
        .ok_or_else(outbox_full)?;
    let data_len = u32::try_from(data.len()).map_err(|_| outbox_full())?;
    file.write_all(&frame_len.to_be_bytes())
        .and_then(|_| file.write_all(&ordinal.to_be_bytes()))
        .and_then(|_| file.write_all(&[kind]))
        .and_then(|_| file.write_all(&data_len.to_be_bytes()))
        .and_then(|_| file.write_all(data))
        .and_then(|_| file.write_all(&previous))
        .and_then(|_| file.write_all(&hash))
        .and_then(|_| file.write_all(&frame_len.to_be_bytes()))
        .map_err(io_error)
}

pub(super) fn new_generation() -> [u8; GENERATION_BYTES] {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let counter = GENERATION_COUNTER.fetch_add(1, Ordering::Relaxed);
    let mut hasher = Sha256::new();
    hasher.update(now.to_be_bytes());
    hasher.update(std::process::id().to_be_bytes());
    hasher.update(counter.to_be_bytes());
    hasher.finalize()[..GENERATION_BYTES]
        .try_into()
        .unwrap_or([0; GENERATION_BYTES])
}

fn header_hash(generation: [u8; GENERATION_BYTES]) -> [u8; HASH_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(MAGIC);
    hasher.update(generation);
    hasher.finalize().into()
}

pub(super) fn record_hash(
    ordinal: u64,
    kind: u8,
    data: &[u8],
    previous: [u8; HASH_BYTES],
) -> [u8; HASH_BYTES] {
    let mut hasher = Sha256::new();
    hasher.update(ordinal.to_be_bytes());
    hasher.update([kind]);
    hasher.update((data.len() as u64).to_be_bytes());
    hasher.update(data);
    hasher.update(previous);
    hasher.finalize().into()
}

pub(super) fn encoded_frame_bytes(data_len: usize) -> SyncResult<u64> {
    validate_record_data_len(data_len)?;
    Ok(4 + FRAME_BODY_FIXED_BYTES as u64 + data_len as u64)
}

pub(super) fn validate_record_data(data: &[u8]) -> SyncResult<()> {
    validate_record_data_len(data.len())
}

fn validate_record_data_len(data_len: usize) -> SyncResult<()> {
    if data_len > MAX_RECORD_DATA_BYTES {
        return Err(outbox_full());
    }
    Ok(())
}

pub(super) fn validate_batch_id(batch_id: &str) -> SyncResult<()> {
    if batch_id.is_empty()
        || batch_id.len() > MAX_BATCH_ID_BYTES
        || batch_id.chars().any(char::is_control)
    {
        return Err(SyncError::InvalidSyncMessage("invalid outbox batch id"));
    }
    Ok(())
}

pub(super) fn ensure_append_capacity(
    current: u64,
    additional: u64,
    reserve: u64,
) -> SyncResult<()> {
    if current
        .checked_add(additional)
        .and_then(|size| size.checked_add(reserve))
        .is_none_or(|size| size > MAX_OUTBOX_FILE_BYTES)
    {
        return Err(outbox_full());
    }
    Ok(())
}

pub(super) fn corrupt_record(ordinal: u64) -> SyncError {
    SyncError::CorruptOutbox {
        line: usize::try_from(ordinal.saturating_add(2)).unwrap_or(usize::MAX),
    }
}

pub(super) fn outbox_full() -> SyncError {
    SyncError::ReplicationFailed("sync outbox exceeds configured limit".to_string())
}

fn io_error(error: std::io::Error) -> SyncError {
    SyncError::ReplicationFailed(error.to_string())
}
