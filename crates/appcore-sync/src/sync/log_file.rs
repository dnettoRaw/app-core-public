// =============================================================================
//        #######
//     ###       ###     F: log_file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! File-backed replication log with incremental scans and offset-only indexes.

use crate::sync::error::{SyncError, SyncResult};
use crate::sync::log::{
    replication_record_hash, validate_log_index, validate_page_limits, validate_record_count,
    validate_record_size, ReplicationLog, MAX_REPLICATION_LOG_BYTES, MAX_REPLICATION_PAGE_BYTES,
    MAX_REPLICATION_PAGE_RECORDS, REPLICATION_LOG_FORMAT_V1,
};
use crate::sync::log_file_format::{
    anchor_matches, append_record, read_header, read_payload, read_payload_from, scan_tail,
    write_record, RecordLocation, TailScan,
};
use crate::sync::persistence::{
    acquire_persistence_lock, atomic_write_with, reject_symlink, truncate_synced,
};
use crate::sync::snapshot::{
    snapshot_from_payloads, validate_snapshot_contract, ReplicationSnapshot,
};
use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};

/// File-backed append-only replication log for local Runtime sync.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReplicationLog {
    file_path: PathBuf,
    state: LogState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LogState {
    records: Vec<RecordLocation>,
    sequence_indices: Vec<(u64, usize)>,
    scanned_bytes: u64,
    record_count: usize,
    line_count: usize,
    chain_head: String,
}

impl FileReplicationLog {
    /// Opens a relative append-only log below `storage_path`.
    pub fn new(storage_path: impl AsRef<Path>, relative_path: &str) -> SyncResult<Self> {
        let file_path = validated_path(storage_path.as_ref(), relative_path)?;
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(replication_error)?;
        }
        let _process_lock = acquire_persistence_lock(&file_path)?;
        if !file_path.exists() {
            create_empty_log(&file_path)?;
        }
        let state = load_state(&file_path)?;
        Ok(Self { file_path, state })
    }

    /// Returns the durable log file path.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Re-reads and validates all durable records from disk.
    pub fn reload(&mut self) -> SyncResult<()> {
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.state = load_state(&self.file_path)?;
        Ok(())
    }

    /// Reads one page after `index`, bounded before payload allocation.
    pub fn events_page(
        &self,
        index: usize,
        max_records: usize,
        max_bytes: usize,
    ) -> SyncResult<Vec<Vec<u8>>> {
        validate_log_index(index, self.state.records.len())?;
        validate_page_limits(max_records, max_bytes)?;
        read_page(
            &self.file_path,
            &self.state.records[index..],
            max_records,
            max_bytes,
        )
    }

    fn refresh_unlocked(&mut self) -> SyncResult<()> {
        let header = read_header(&self.file_path)?;
        if header.incomplete || header.file_bytes < self.state.scanned_bytes {
            self.state = load_state(&self.file_path)?;
            return Ok(());
        }
        if !anchor_matches(
            &self.file_path,
            self.state.records.last(),
            &self.state.chain_head,
        )? {
            self.state = load_state(&self.file_path)?;
            return Ok(());
        }
        if header.file_bytes == self.state.scanned_bytes {
            return Ok(());
        }
        let tail = scan_tail(
            &self.file_path,
            self.state.scanned_bytes,
            self.state.record_count,
            self.state.line_count,
            &self.state.chain_head,
            &self.state.records,
            &self.state.sequence_indices,
        )?;
        apply_tail(&mut self.state, tail, &self.file_path)
    }

    fn append_record(&mut self, payload: Vec<u8>, sequence: u64) -> SyncResult<usize> {
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        self.refresh_unlocked()?;
        validate_record_size(&payload)?;
        validate_record_count(self.state.record_count.saturating_add(1))?;
        if sequence > 0 {
            if let Some(index) = find_sequence(&self.state.sequence_indices, sequence) {
                return if read_payload(&self.file_path, &self.state.records[index])? == payload {
                    Ok(index + 1)
                } else {
                    Err(SyncError::SequenceConflict(sequence))
                };
            }
        }
        let previous_hash = self.state.chain_head.clone();
        let record_hash = replication_record_hash(&previous_hash, sequence, &payload);
        let (location, next_offset) = append_record(
            &self.file_path,
            self.state.scanned_bytes,
            sequence,
            &payload,
            &previous_hash,
            &record_hash,
        )?;
        let index = self.state.records.len();
        self.state.records.push(location);
        if sequence > 0 {
            insert_sequence(&mut self.state.sequence_indices, sequence, index);
        }
        self.state.scanned_bytes = next_offset;
        self.state.record_count += 1;
        self.state.line_count += 1;
        self.state.chain_head = record_hash;
        Ok(index + 1)
    }
}

impl ReplicationLog for FileReplicationLog {
    fn append(&mut self, record: Vec<u8>) -> SyncResult<usize> {
        self.append_record(record, 0)
    }

    fn append_with_sequence(&mut self, record: Vec<u8>, sequence: u64) -> SyncResult<usize> {
        self.append_record(record, sequence)
    }

    fn event_at_sequence(&self, sequence: u64) -> SyncResult<Option<Vec<u8>>> {
        if sequence == 0 {
            return Ok(None);
        }
        find_sequence(&self.state.sequence_indices, sequence)
            .map(|index| read_payload(&self.file_path, &self.state.records[index]))
            .transpose()
    }

    fn events_since(&self, index: usize) -> SyncResult<Vec<Vec<u8>>> {
        validate_log_index(index, self.state.records.len())?;
        let records = &self.state.records[index..];
        if records.len() > MAX_REPLICATION_PAGE_RECORDS
            || aggregate_payload_bytes(records)? > MAX_REPLICATION_PAGE_BYTES
        {
            return Err(replication_message(
                "complete replication read exceeds page limits; use events_page",
            ));
        }
        read_page(
            &self.file_path,
            records,
            MAX_REPLICATION_PAGE_RECORDS,
            MAX_REPLICATION_PAGE_BYTES,
        )
    }

    fn events_page(
        &self,
        index: usize,
        max_records: usize,
        max_bytes: usize,
    ) -> SyncResult<Vec<Vec<u8>>> {
        Self::events_page(self, index, max_records, max_bytes)
    }

    fn last_index(&self) -> SyncResult<usize> {
        Ok(self.state.records.len())
    }

    fn len(&self) -> SyncResult<usize> {
        Ok(self.state.records.len())
    }

    fn is_empty(&self) -> SyncResult<bool> {
        Ok(self.state.records.is_empty())
    }

    fn create_snapshot(&self) -> SyncResult<ReplicationSnapshot> {
        let mut file = open_log(&self.file_path)?;
        snapshot_from_payloads(self.state.records.iter().map(|location| {
            read_payload_from(&mut file, location).map(|payload| (location.sequence, payload))
        }))
    }

    fn restore_snapshot(&mut self, snapshot: &ReplicationSnapshot) -> SyncResult<()> {
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        validate_snapshot_contract(snapshot)?;
        write_snapshot(&self.file_path, snapshot)?;
        self.state = load_state(&self.file_path)?;
        Ok(())
    }
}

fn load_state(path: &Path) -> SyncResult<LogState> {
    let header = read_header(path)?;
    if header.incomplete {
        create_empty_log(path)?;
        return Ok(empty_state());
    }
    let tail = scan_tail(path, header.body_offset, 0, 0, "", &[], &[])?;
    let mut state = empty_state();
    apply_tail(&mut state, tail, path)?;
    Ok(state)
}

fn apply_tail(state: &mut LogState, tail: TailScan, path: &Path) -> SyncResult<()> {
    let start = state.records.len();
    for (offset, location) in tail.locations.iter().enumerate() {
        if location.sequence > 0 {
            insert_sequence(
                &mut state.sequence_indices,
                location.sequence,
                start + offset,
            );
        }
    }
    state.records.extend(tail.locations);
    state.scanned_bytes = tail.scanned_bytes;
    state.record_count = tail.record_count;
    state.line_count = tail.line_count;
    state.chain_head = tail.chain_head;
    if tail.recovered_tail {
        truncate_synced(path, state.scanned_bytes)?;
    }
    Ok(())
}

fn read_page(
    path: &Path,
    records: &[RecordLocation],
    max_records: usize,
    max_bytes: usize,
) -> SyncResult<Vec<Vec<u8>>> {
    let mut file = open_log(path)?;
    let mut page = Vec::with_capacity(records.len().min(max_records));
    let mut bytes = 0usize;
    for location in records.iter().take(max_records) {
        let next = bytes
            .checked_add(location.payload_bytes as usize)
            .ok_or_else(|| replication_message("replication page overflow"))?;
        if next > max_bytes {
            if page.is_empty() {
                return Err(replication_message("replication page byte limit too small"));
            }
            break;
        }
        page.push(read_payload_from(&mut file, location)?);
        bytes = next;
    }
    Ok(page)
}

fn aggregate_payload_bytes(records: &[RecordLocation]) -> SyncResult<usize> {
    records.iter().try_fold(0usize, |total, location| {
        total
            .checked_add(location.payload_bytes as usize)
            .ok_or_else(|| replication_message("replication payload byte count overflow"))
    })
}

fn write_snapshot(path: &Path, snapshot: &ReplicationSnapshot) -> SyncResult<()> {
    atomic_write_with(path, |file| {
        write_header(file)?;
        let mut offset = (REPLICATION_LOG_FORMAT_V1.len() + 1) as u64;
        let mut previous_hash = String::new();
        for record in &snapshot.records {
            let hash = replication_record_hash(&previous_hash, record.sequence, &record.payload);
            let (_, written_bytes) = write_record(
                file,
                offset,
                record.sequence,
                &record.payload,
                &previous_hash,
                &hash,
            )?;
            offset = offset
                .checked_add(written_bytes)
                .filter(|bytes| *bytes <= MAX_REPLICATION_LOG_BYTES)
                .ok_or_else(|| replication_message("replication log exceeds configured limit"))?;
            previous_hash = hash;
        }
        Ok(())
    })
}

fn create_empty_log(path: &Path) -> SyncResult<()> {
    atomic_write_with(path, write_header)
}

fn write_header(file: &mut File) -> SyncResult<()> {
    file.write_all(REPLICATION_LOG_FORMAT_V1.as_bytes())
        .and_then(|()| file.write_all(b"\n"))
        .map_err(replication_error)
}

fn empty_state() -> LogState {
    LogState {
        records: Vec::new(),
        sequence_indices: Vec::new(),
        scanned_bytes: (REPLICATION_LOG_FORMAT_V1.len() + 1) as u64,
        record_count: 0,
        line_count: 0,
        chain_head: String::new(),
    }
}

fn find_sequence(index: &[(u64, usize)], sequence: u64) -> Option<usize> {
    index
        .binary_search_by_key(&sequence, |(key, _)| *key)
        .ok()
        .map(|position| index[position].1)
}

fn insert_sequence(index: &mut Vec<(u64, usize)>, sequence: u64, record_index: usize) {
    match index.binary_search_by_key(&sequence, |(key, _)| *key) {
        Ok(position) => index[position] = (sequence, record_index),
        Err(position) => index.insert(position, (sequence, record_index)),
    }
}

fn validated_path(storage_root: &Path, relative_path: &str) -> SyncResult<PathBuf> {
    let relative = PathBuf::from(relative_path);
    if relative.as_os_str().is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return Err(replication_message("invalid replication log path"));
    }
    Ok(storage_root.join(relative))
}

fn open_log(path: &Path) -> SyncResult<File> {
    reject_symlink(path)?;
    File::open(path).map_err(replication_error)
}

fn replication_message(message: &str) -> SyncError {
    SyncError::ReplicationFailed(message.to_string())
}

fn replication_error(error: std::io::Error) -> SyncError {
    SyncError::ReplicationFailed(error.to_string())
}
