// =============================================================================
//        #######
//     ###       ###     F: outbox_journal_view.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Bounded payload views over file outbox journal state.

use crate::sync::error::{SyncError, SyncResult};
use crate::sync::outbox::{SyncOutboxReceipt, SyncOutboxStats};
use crate::sync::outbox_format::{corrupt_record, io_error, validate_batch_id};
use crate::sync::types::SyncMessage;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::Arc;

pub(super) struct PendingMessage {
    pub(super) batch_id: Arc<str>,
    pub(super) ordinal: u64,
    pub(super) data_offset: u64,
    pub(super) data_digest: [u8; 32],
    pub(super) encoded_bytes: usize,
    pub(super) frame_bytes: u64,
    pub(super) attempt_frame_bytes: u64,
    pub(super) attempts: u32,
    pub(super) next_ready_at_ms: u64,
}

pub(super) fn load_message(path: &Path, pending: &PendingMessage) -> SyncResult<SyncMessage> {
    let mut file = File::open(path).map_err(io_error)?;
    file.seek(SeekFrom::Start(pending.data_offset))
        .map_err(io_error)?;
    let limited = BufReader::new(file).take(pending.encoded_bytes as u64);
    let mut reader = HashingReader::new(limited);
    let result = {
        let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
        let message = SyncMessage::deserialize(&mut deserializer);
        message.and_then(|message| {
            deserializer.end()?;
            Ok(message)
        })
    };
    let (read_bytes, digest) = reader.finish();
    if read_bytes != pending.encoded_bytes as u64 || digest != pending.data_digest {
        return Err(corrupt_record(pending.ordinal));
    }
    let message = result.map_err(|_| corrupt_record(pending.ordinal))?;
    validate_batch_id(&message.batch_id).map_err(|_| corrupt_record(pending.ordinal))?;
    if message.batch_id != pending.batch_id.as_ref() {
        return Err(corrupt_record(pending.ordinal));
    }
    Ok(message)
}

struct HashingReader<R> {
    inner: R,
    hasher: Sha256,
    read_bytes: u64,
}

impl<R> HashingReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            hasher: Sha256::new(),
            read_bytes: 0,
        }
    }

    fn finish(self) -> (u64, [u8; 32]) {
        (self.read_bytes, self.hasher.finalize().into())
    }
}

impl<R: Read> Read for HashingReader<R> {
    fn read(&mut self, output: &mut [u8]) -> std::io::Result<usize> {
        let read = self.inner.read(output)?;
        self.hasher.update(&output[..read]);
        self.read_bytes = self.read_bytes.saturating_add(read as u64);
        Ok(read)
    }
}

pub(super) fn page(
    path: &Path,
    messages: &VecDeque<PendingMessage>,
    limit: usize,
    max_bytes: usize,
    ready_at_ms: Option<u64>,
) -> SyncResult<Vec<SyncMessage>> {
    let mut page = Vec::new();
    let mut bytes = 0usize;
    for pending in messages.iter().take(limit) {
        if ready_at_ms.is_some_and(|now| pending.next_ready_at_ms > now)
            || bytes
                .checked_add(pending.encoded_bytes)
                .is_none_or(|total| total > max_bytes)
        {
            break;
        }
        bytes += pending.encoded_bytes;
        page.push(load_message(path, pending)?);
    }
    Ok(page)
}

pub(super) fn stats(messages: &VecDeque<PendingMessage>) -> SyncResult<SyncOutboxStats> {
    let pending_bytes = messages.iter().try_fold(0usize, |total, pending| {
        total
            .checked_add(pending.encoded_bytes)
            .ok_or(SyncError::InvalidSyncMessage("outbox byte overflow"))
    })?;
    let total_attempts = messages.iter().try_fold(0u64, |total, pending| {
        total
            .checked_add(u64::from(pending.attempts))
            .ok_or(SyncError::InvalidSyncMessage("outbox attempt overflow"))
    })?;
    Ok(SyncOutboxStats {
        pending_messages: messages.len(),
        pending_bytes: Some(pending_bytes),
        attempted_messages: Some(
            messages
                .iter()
                .filter(|pending| pending.attempts > 0)
                .count(),
        ),
        total_attempts: Some(total_attempts),
        next_ready_at_ms: messages.front().map(|pending| pending.next_ready_at_ms),
    })
}

pub(super) fn validate_receipt_prefix(
    messages: &VecDeque<PendingMessage>,
    receipt: &SyncOutboxReceipt,
) -> SyncResult<()> {
    if messages.len() < receipt.batch_ids().len()
        || messages
            .iter()
            .zip(receipt.batch_ids())
            .any(|(pending, batch_id)| pending.batch_id.as_ref() != batch_id)
    {
        return Err(SyncError::InvalidSyncMessage(
            "outbox acknowledgement mismatch",
        ));
    }
    Ok(())
}
