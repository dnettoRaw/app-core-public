// =============================================================================
//        #######
//     ###       ###     F: log.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Replication log contracts and local implementations.

use crate::sync::codec::bytes_to_hex;
use crate::sync::error::{SyncError, SyncResult};
use crate::sync::snapshot::{
    into_replication_records, snapshot_from_records, validate_snapshot, ReplicationSnapshot,
};
use sha2::{Digest, Sha256};
use std::borrow::Cow;

pub use crate::sync::log_file::FileReplicationLog;

/// Stable on-disk format marker for hash-chained replication logs.
pub const REPLICATION_LOG_FORMAT_V1: &str = "# appcore-replication-log-v1";
pub(super) const MAX_REPLICATION_LOG_BYTES: u64 = 256 * 1024 * 1024;
pub(super) const MAX_REPLICATION_RECORD_BYTES: usize = 1024 * 1024;
pub(super) const MAX_REPLICATION_RECORDS: usize = 262_144;
/// Maximum records returned by one bounded replication-log page.
pub const MAX_REPLICATION_PAGE_RECORDS: usize = 1024;
/// Maximum aggregate payload bytes returned by one replication-log page.
pub const MAX_REPLICATION_PAGE_BYTES: usize = 48 * 1024 * 1024;
/// Maximum raw event bytes grouped into one Runtime HTTP sync batch.
pub const MAX_SYNC_BATCH_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Replication log contract.
pub trait ReplicationLog {
    /// Appends an unsequenced record and returns its one-based log index.
    fn append(&mut self, record: Vec<u8>) -> SyncResult<usize>;
    /// Idempotently appends `record` at a source sequence.
    fn append_with_sequence(&mut self, record: Vec<u8>, sequence: u64) -> SyncResult<usize>;
    /// Returns the payload at a source sequence when sequence lookup is supported.
    fn event_at_sequence(&self, _sequence: u64) -> SyncResult<Option<Vec<u8>>> {
        Ok(None)
    }
    /// Returns payloads after the supplied zero-based log offset.
    fn events_since(&self, index: usize) -> SyncResult<Vec<Vec<u8>>>;
    /// Returns one payload page with bounded record count and payload bytes.
    ///
    /// Compatibility providers may use the default full-read adapter. Durable
    /// providers should override this method to enforce both limits before
    /// materializing payloads.
    /// The default moves selected payloads without another deep copy, but cannot
    /// bound the allocation performed by `events_since` before selection.
    fn events_page(
        &self,
        index: usize,
        max_records: usize,
        max_bytes: usize,
    ) -> SyncResult<Vec<Vec<u8>>> {
        validate_page_limits(max_records, max_bytes)?;
        let events = self.events_since(index)?;
        bounded_page(events.into_iter().map(Cow::Owned), max_records, max_bytes)
    }
    /// Returns the one-based final log index, or zero for an empty log.
    fn last_index(&self) -> SyncResult<usize>;
    /// Returns the number of records in the log.
    fn len(&self) -> SyncResult<usize>;
    /// Reports whether the log contains no records.
    fn is_empty(&self) -> SyncResult<bool>;
    /// Creates a validated portable snapshot when supported.
    fn create_snapshot(&self) -> SyncResult<ReplicationSnapshot> {
        Err(SyncError::SnapshotUnsupported)
    }
    /// Atomically replaces log contents from a validated snapshot when supported.
    fn restore_snapshot(&mut self, _snapshot: &ReplicationSnapshot) -> SyncResult<()> {
        Err(SyncError::SnapshotUnsupported)
    }
}

/// In-memory replication log for local sync scenarios.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct InMemoryReplicationLog {
    events: Vec<ReplicationRecord>,
    /// Sorted sequence-to-event offsets; a flat index avoids hash-bucket
    /// overhead while retaining logarithmic lookup.
    sequence_indices: Vec<(u64, usize)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct ReplicationRecord {
    pub(super) index: usize,
    pub(super) sequence: u64,
    pub(super) payload: Vec<u8>,
    pub(super) previous_hash: String,
    pub(super) record_hash: String,
}

impl InMemoryReplicationLog {
    /// Creates an empty process-local replication log.
    pub fn new() -> Self {
        Self {
            events: Vec::new(),
            sequence_indices: Vec::new(),
        }
    }

    /// Idempotently appends a record at a source sequence.
    pub fn append_with_sequence(&mut self, record: Vec<u8>, sequence: u64) -> SyncResult<usize> {
        validate_record_size(&record)?;
        validate_record_count(self.events.len().saturating_add(1))?;
        if sequence > 0 {
            if let Some(existing) = self
                .sequence_offset(sequence)
                .and_then(|offset| self.events.get(offset))
            {
                return if existing.payload == record {
                    Ok(existing.index)
                } else {
                    Err(SyncError::SequenceConflict(sequence))
                };
            }
        }
        let index = self.events.len() + 1;
        let previous_hash = self
            .events
            .last()
            .map(|record| record.record_hash.clone())
            .unwrap_or_default();
        let record_hash = replication_record_hash(&previous_hash, sequence, &record);
        self.events.push(ReplicationRecord {
            index,
            sequence,
            payload: record,
            previous_hash,
            record_hash,
        });
        insert_sequence_index(&mut self.sequence_indices, sequence, index - 1);
        Ok(index)
    }

    /// Returns the one-based final log index, or zero when empty.
    pub fn last_index(&self) -> usize {
        self.events.len()
    }

    /// Reports whether a source sequence is present.
    pub fn contains_sequence(&self, sequence: u64) -> bool {
        self.sequence_offset(sequence).is_some()
    }

    fn sequence_offset(&self, sequence: u64) -> Option<usize> {
        self.sequence_indices
            .binary_search_by_key(&sequence, |(value, _)| *value)
            .ok()
            .map(|position| self.sequence_indices[position].1)
    }

    /// Restores a snapshot by moving its payload allocations into the log.
    ///
    /// This consuming variant avoids retaining the source snapshot and the
    /// destination record payloads at the same time. Use the trait method
    /// [`ReplicationLog::restore_snapshot`] when the caller must keep a
    /// borrowed snapshot for compatibility.
    pub fn restore_snapshot_owned(&mut self, snapshot: ReplicationSnapshot) -> SyncResult<()> {
        let records = into_replication_records(snapshot)?;
        self.sequence_indices = sequence_indices(&records);
        self.events = records;
        Ok(())
    }
}

impl ReplicationLog for InMemoryReplicationLog {
    fn append(&mut self, record: Vec<u8>) -> SyncResult<usize> {
        self.append_with_sequence(record, 0)
    }

    fn append_with_sequence(&mut self, record: Vec<u8>, sequence: u64) -> SyncResult<usize> {
        self.append_with_sequence(record, sequence)
    }

    fn event_at_sequence(&self, sequence: u64) -> SyncResult<Option<Vec<u8>>> {
        Ok(self
            .sequence_offset(sequence)
            .filter(|_| sequence > 0)
            .and_then(|offset| self.events.get(offset))
            .map(|event| event.payload.clone()))
    }

    fn events_since(&self, index: usize) -> SyncResult<Vec<Vec<u8>>> {
        if index > self.events.len() {
            return Err(SyncError::LogIndexOutOfBounds {
                index,
                len: self.events.len(),
            });
        }
        Ok(self.events[index..]
            .iter()
            .map(|record| record.payload.clone())
            .collect::<Vec<_>>())
    }

    fn events_page(
        &self,
        index: usize,
        max_records: usize,
        max_bytes: usize,
    ) -> SyncResult<Vec<Vec<u8>>> {
        validate_log_index(index, self.events.len())?;
        validate_page_limits(max_records, max_bytes)?;
        bounded_page(
            self.events[index..]
                .iter()
                .map(|record| Cow::Borrowed(record.payload.as_slice())),
            max_records,
            max_bytes,
        )
    }

    fn len(&self) -> SyncResult<usize> {
        Ok(self.events.len())
    }

    fn last_index(&self) -> SyncResult<usize> {
        Ok(self.last_index())
    }

    fn is_empty(&self) -> SyncResult<bool> {
        Ok(self.events.is_empty())
    }

    fn create_snapshot(&self) -> SyncResult<ReplicationSnapshot> {
        Ok(snapshot_from_records(&self.events))
    }

    fn restore_snapshot(&mut self, snapshot: &ReplicationSnapshot) -> SyncResult<()> {
        let records = validate_snapshot(snapshot)?;
        self.sequence_indices = sequence_indices(&records);
        self.events = records;
        Ok(())
    }
}

fn sequence_indices(records: &[ReplicationRecord]) -> Vec<(u64, usize)> {
    let mut indices = Vec::with_capacity(records.len());
    for (offset, record) in records.iter().enumerate() {
        insert_sequence_index(&mut indices, record.sequence, offset);
    }
    indices
}

fn insert_sequence_index(indices: &mut Vec<(u64, usize)>, sequence: u64, offset: usize) {
    match indices.binary_search_by_key(&sequence, |(value, _)| *value) {
        Ok(position) => indices[position] = (sequence, offset),
        Err(position) => indices.insert(position, (sequence, offset)),
    }
}

pub(super) fn validate_record_size(payload: &[u8]) -> SyncResult<()> {
    if payload.len() > MAX_REPLICATION_RECORD_BYTES {
        return Err(SyncError::ReplicationFailed(
            "replication record exceeds size limit".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_record_count(record_count: usize) -> SyncResult<()> {
    if record_count > MAX_REPLICATION_RECORDS {
        return Err(SyncError::ReplicationFailed(
            "replication record limit exceeded".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_page_limits(max_records: usize, max_bytes: usize) -> SyncResult<()> {
    if max_records == 0
        || max_records > MAX_REPLICATION_PAGE_RECORDS
        || max_bytes == 0
        || max_bytes > MAX_REPLICATION_PAGE_BYTES
    {
        return Err(SyncError::ReplicationFailed(
            "invalid replication page limits".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_log_index(index: usize, len: usize) -> SyncResult<()> {
    if index > len {
        return Err(SyncError::LogIndexOutOfBounds { index, len });
    }
    Ok(())
}

fn bounded_page<'a>(
    payloads: impl Iterator<Item = Cow<'a, [u8]>>,
    max_records: usize,
    max_bytes: usize,
) -> SyncResult<Vec<Vec<u8>>> {
    let mut page = Vec::with_capacity(max_records);
    let mut bytes = 0usize;
    for payload in payloads.take(max_records) {
        let next = bytes
            .checked_add(payload.len())
            .ok_or_else(|| SyncError::ReplicationFailed("replication page overflow".to_string()))?;
        if next > max_bytes {
            if page.is_empty() {
                return Err(SyncError::ReplicationFailed(
                    "replication page byte limit too small".to_string(),
                ));
            }
            break;
        }
        bytes = next;
        page.push(payload.into_owned());
    }
    Ok(page)
}

pub(super) fn replication_record_hash(
    previous_hash: &str,
    sequence: u64,
    payload: &[u8],
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(REPLICATION_LOG_FORMAT_V1.as_bytes());
    hasher.update((previous_hash.len() as u64).to_be_bytes());
    hasher.update(previous_hash.as_bytes());
    hasher.update(sequence.to_be_bytes());
    hasher.update((payload.len() as u64).to_be_bytes());
    hasher.update(payload);
    bytes_to_hex(&hasher.finalize())
}
