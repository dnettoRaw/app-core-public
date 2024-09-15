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

use crate::sync::codec::{bytes_to_hex, hex_to_bytes};
use crate::sync::error::{SyncError, SyncResult};
use crate::sync::persistence::{
    acquire_persistence_lock, atomic_write, read_bounded_text, split_format,
};
use crate::sync::snapshot::{snapshot_from_records, validate_snapshot, ReplicationSnapshot};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Stable on-disk format marker for hash-chained replication logs.
pub const REPLICATION_LOG_FORMAT_V1: &str = "# appcore-replication-log-v1";
const MAX_REPLICATION_LOG_BYTES: u64 = 256 * 1024 * 1024;
const MAX_REPLICATION_RECORD_BYTES: usize = 1024 * 1024;

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
    /// Returns the one-based final log index, or zero for an empty log.
    fn last_index(&self) -> usize;
    /// Returns the number of records in the log.
    fn len(&self) -> usize;
    /// Reports whether the log contains no records.
    fn is_empty(&self) -> bool;
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
    sequence_indices: HashMap<u64, usize>,
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
            sequence_indices: HashMap::new(),
        }
    }

    /// Idempotently appends a record at a source sequence.
    pub fn append_with_sequence(&mut self, record: Vec<u8>, sequence: u64) -> SyncResult<usize> {
        validate_record_size(&record)?;
        if sequence > 0 {
            if let Some(existing) = self
                .sequence_indices
                .get(&sequence)
                .and_then(|offset| self.events.get(*offset))
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
        self.sequence_indices.insert(sequence, index - 1);
        Ok(index)
    }

    /// Returns the one-based final log index, or zero when empty.
    pub fn last_index(&self) -> usize {
        self.events.len()
    }

    /// Reports whether a source sequence is present.
    pub fn contains_sequence(&self, sequence: u64) -> bool {
        self.sequence_indices.contains_key(&sequence)
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
            .sequence_indices
            .get(&sequence)
            .filter(|_| sequence > 0)
            .and_then(|offset| self.events.get(*offset))
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

    fn len(&self) -> usize {
        self.events.len()
    }

    fn last_index(&self) -> usize {
        self.last_index()
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
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

/// File-backed append-only replication log for local runtime sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReplicationLog {
    file_path: PathBuf,
    events: Vec<ReplicationRecord>,
    sequence_indices: HashMap<u64, usize>,
}

impl FileReplicationLog {
    /// Opens a relative append-only log below `storage_path`.
    pub fn new(storage_path: impl AsRef<Path>, relative_path: &str) -> SyncResult<Self> {
        let storage_root = storage_path.as_ref();
        let relative = PathBuf::from(relative_path);
        if relative.as_os_str().is_empty()
            || relative.is_absolute()
            || relative
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(SyncError::ReplicationFailed(
                "invalid replication log path".to_string(),
            ));
        }
        let file_path = storage_root.join(&relative);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| SyncError::ReplicationFailed(err.to_string()))?;
        }
        let _process_lock = acquire_persistence_lock(&file_path)?;
        if !file_path.exists() {
            atomic_write(
                &file_path,
                format!("{REPLICATION_LOG_FORMAT_V1}\n").as_bytes(),
            )?;
        }
        let mut log = Self {
            file_path,
            events: Vec::new(),
            sequence_indices: HashMap::new(),
        };
        log.reload_unlocked()?;
        Ok(log)
    }

    /// Returns the durable log file path.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Re-reads and validates all durable records from disk.
    pub fn reload(&mut self) -> SyncResult<()> {
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.reload_unlocked()
    }

    fn reload_unlocked(&mut self) -> SyncResult<()> {
        let text = read_bounded_text(&self.file_path, MAX_REPLICATION_LOG_BYTES)?;
        let formatted = split_format(&text, REPLICATION_LOG_FORMAT_V1)?;
        let (records, recovered_tail) = parse_replication_records(formatted.body)?;
        if recovered_tail {
            write_replication_records(&self.file_path, &records)?;
        }
        self.sequence_indices = sequence_indices(&records);
        self.events = records;
        Ok(())
    }
}

impl ReplicationLog for FileReplicationLog {
    fn append(&mut self, record: Vec<u8>) -> SyncResult<usize> {
        self.append_with_sequence(record, 0)
    }

    fn append_with_sequence(&mut self, record: Vec<u8>, sequence: u64) -> SyncResult<usize> {
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.reload_unlocked()?;
        validate_record_size(&record)?;
        if sequence > 0 {
            if let Some(existing) = self
                .sequence_indices
                .get(&sequence)
                .and_then(|offset| self.events.get(*offset))
            {
                return if existing.payload == record {
                    Ok(existing.index)
                } else {
                    Err(SyncError::SequenceConflict(sequence))
                };
            }
        }
        let previous_hash = self
            .events
            .last()
            .map(|entry| entry.record_hash.clone())
            .unwrap_or_default();
        let record_hash = replication_record_hash(&previous_hash, sequence, &record);
        let line = format!(
            "{}\t{}\t{}\t{}\n",
            sequence,
            bytes_to_hex(&record),
            previous_hash,
            record_hash
        );
        let current_bytes = fs::metadata(&self.file_path)
            .map_err(|err| SyncError::ReplicationFailed(err.to_string()))?
            .len();
        if current_bytes
            .checked_add(line.len() as u64)
            .is_none_or(|bytes| bytes > MAX_REPLICATION_LOG_BYTES)
        {
            return Err(SyncError::ReplicationFailed(
                "replication log exceeds configured limit".to_string(),
            ));
        }
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(&self.file_path)
            .map_err(|err| SyncError::ReplicationFailed(err.to_string()))?;
        file.write_all(line.as_bytes())
            .map_err(|err| SyncError::ReplicationFailed(err.to_string()))?;
        file.sync_data()
            .map_err(|err| SyncError::ReplicationFailed(err.to_string()))?;
        let index = self.events.len() + 1;
        self.events.push(ReplicationRecord {
            index,
            sequence,
            payload: record,
            previous_hash,
            record_hash,
        });
        self.sequence_indices.insert(sequence, index - 1);
        Ok(index)
    }

    fn event_at_sequence(&self, sequence: u64) -> SyncResult<Option<Vec<u8>>> {
        Ok(self
            .sequence_indices
            .get(&sequence)
            .filter(|_| sequence > 0)
            .and_then(|offset| self.events.get(*offset))
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

    fn last_index(&self) -> usize {
        self.events.len()
    }

    fn len(&self) -> usize {
        self.events.len()
    }

    fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    fn create_snapshot(&self) -> SyncResult<ReplicationSnapshot> {
        Ok(snapshot_from_records(&self.events))
    }

    fn restore_snapshot(&mut self, snapshot: &ReplicationSnapshot) -> SyncResult<()> {
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        let records = validate_snapshot(snapshot)?;
        write_replication_records(&self.file_path, &records)?;
        self.sequence_indices = sequence_indices(&records);
        self.events = records;
        Ok(())
    }
}

fn sequence_indices(records: &[ReplicationRecord]) -> HashMap<u64, usize> {
    records
        .iter()
        .enumerate()
        .map(|(offset, record)| (record.sequence, offset))
        .collect()
}

fn parse_replication_records(body: &str) -> SyncResult<(Vec<ReplicationRecord>, bool)> {
    let (complete, recovered_tail) = complete_record_prefix(body);
    let mut records = Vec::<ReplicationRecord>::new();
    let mut sequences = HashMap::<u64, usize>::new();
    let mut chain_head = String::new();
    for (line_number, line) in complete.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() != 4 {
            let reason = if line.contains('\t') {
                "invalid field count"
            } else {
                "missing separator"
            };
            return Err(corrupt_log(line_number, reason));
        }
        let sequence = parts[0]
            .parse::<u64>()
            .map_err(|_| corrupt_log(line_number, "invalid sequence"))?;
        let payload =
            hex_to_bytes(parts[1]).map_err(|_| corrupt_log(line_number, "invalid event hex"))?;
        validate_record_size(&payload)?;
        let record_hash = replication_record_hash(&chain_head, sequence, &payload);
        if parts[2] != chain_head || parts[3] != record_hash {
            return Err(corrupt_log(line_number, "hash chain mismatch"));
        }
        if sequence > 0 {
            if let Some(offset) = sequences.get(&sequence) {
                if records[*offset].payload != payload {
                    return Err(SyncError::SequenceConflict(sequence));
                }
                return Err(corrupt_log(line_number, "duplicate sequence"));
            }
        }
        let index = records.len() + 1;
        records.push(ReplicationRecord {
            index,
            sequence,
            payload,
            previous_hash: chain_head,
            record_hash: record_hash.clone(),
        });
        sequences.insert(sequence, index - 1);
        chain_head = record_hash;
    }
    Ok((records, recovered_tail))
}

fn complete_record_prefix(body: &str) -> (&str, bool) {
    if body.is_empty() || body.ends_with('\n') {
        return (body, false);
    }
    match body.rfind('\n') {
        Some(last_newline) => (&body[..=last_newline], true),
        None => ("", true),
    }
}

fn corrupt_log(line_number: usize, reason: &'static str) -> SyncError {
    SyncError::CorruptReplicationLog {
        line: line_number + 1,
        reason,
    }
}

fn write_replication_records(path: &Path, records: &[ReplicationRecord]) -> SyncResult<()> {
    let mut output = format!("{REPLICATION_LOG_FORMAT_V1}\n");
    let mut previous_hash = String::new();
    for record in records {
        validate_record_size(&record.payload)?;
        let hash = replication_record_hash(&previous_hash, record.sequence, &record.payload);
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            record.sequence,
            bytes_to_hex(&record.payload),
            previous_hash,
            hash
        ));
        if output.len() as u64 > MAX_REPLICATION_LOG_BYTES {
            return Err(SyncError::ReplicationFailed(
                "replication log exceeds configured limit".to_string(),
            ));
        }
        previous_hash = hash;
    }
    atomic_write(path, output.as_bytes())
}

pub(super) fn validate_record_size(payload: &[u8]) -> SyncResult<()> {
    if payload.len() > MAX_REPLICATION_RECORD_BYTES {
        return Err(SyncError::ReplicationFailed(
            "replication record exceeds size limit".to_string(),
        ));
    }
    Ok(())
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
