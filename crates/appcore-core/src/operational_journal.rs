// =============================================================================
//        #######
//     ###       ###     F: operational_journal.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Durable bounded journal for generic audit entries and emitted events.

#[cfg(test)]
use crate::operational_journal_encoding::encoded_records_bytes;
use crate::operational_journal_encoding::{
    append_envelope, encode_records, record_hash, retained_suffix_count, write_header,
};
use crate::{AuditEntry, EventEnvelope, RuntimeError, RuntimeResult};
use fs2::FileExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Stable format marker for the operational journal.
pub const OPERATIONAL_JOURNAL_FORMAT_V1: &str = "# appcore-operational-journal-v1";
pub(super) const MAX_JOURNAL_RECORD_BYTES: usize = 1024 * 1024;
const MAX_JOURNAL_ENVELOPE_BYTES: usize = MAX_JOURNAL_RECORD_BYTES + 1024;
// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static JOURNAL_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// One persisted operational record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_type", content = "record", rename_all = "snake_case")]
pub enum OperationalJournalRecord {
    /// Generic Runtime audit entry.
    Audit(AuditEntry),
    /// Opaque event envelope emitted by an application command.
    Event(EventEnvelope),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalEnvelope {
    sequence: u64,
    previous_hash: String,
    hash: String,
    record: OperationalJournalRecord,
}

enum LineStatus {
    Complete,
    Partial,
    End,
}

struct JournalState {
    records: Arc<VecDeque<Arc<OperationalJournalRecord>>>,
    sequence: u64,
    last_hash: String,
}

/// Process-locked, hash-chained operational journal.
pub struct FileOperationalJournal {
    path: PathBuf,
    _lock: File,
    max_records: usize,
    max_bytes: u64,
    state: Mutex<JournalState>,
}

impl std::fmt::Debug for FileOperationalJournal {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FileOperationalJournal")
            .field("path", &self.path)
            .field("max_records", &self.max_records)
            .field("max_bytes", &self.max_bytes)
            .field("record_count", &self.state.lock().records.len())
            .finish()
    }
}

impl FileOperationalJournal {
    /// Opens a journal, validates its hash chain, and sanitizes stored audit text.
    pub fn open(
        path: impl Into<PathBuf>,
        max_records: usize,
        max_bytes: u64,
    ) -> RuntimeResult<Self> {
        let path = path.into();
        let parent = path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|error| journal_io("create_parent", error))?;
        reject_symlink(&path)?;
        let lock = open_lock(&path.with_extension("journal.lock"))?;
        lock.try_lock_exclusive()
            .map_err(|error| journal_io("lock", error))?;
        if !path.exists() {
            atomic_replace(&path, write_header)?;
        }
        let configured_max_bytes = max_bytes.max(1);
        let recovery_max_bytes = configured_max_bytes
            .saturating_add(MAX_JOURNAL_RECORD_BYTES as u64)
            .saturating_add(64 * 1024);
        let (state, recovered_tail) = load_state(&path, recovery_max_bytes)?;
        let journal = Self {
            path,
            _lock: lock,
            max_records: max_records.max(1),
            max_bytes: configured_max_bytes,
            state: Mutex::new(state),
        };
        let exceeds_limits = {
            let state = journal.state.lock();
            state.records.len() > journal.max_records
                || fs::metadata(&journal.path)
                    .map(|metadata| metadata.len() > journal.max_bytes)
                    .unwrap_or(true)
        };
        if recovered_tail || exceeds_limits {
            journal.compact_locked(&mut journal.state.lock())?;
        }
        Ok(journal)
    }

    /// Appends one redacted audit entry.
    pub fn append_audit(&self, entry: AuditEntry) -> RuntimeResult<()> {
        self.append(OperationalJournalRecord::Audit(entry.into_bounded()))
    }

    /// Appends one opaque event envelope.
    pub fn append_event(&self, event: EventEnvelope) -> RuntimeResult<()> {
        self.append(OperationalJournalRecord::Event(event))
    }

    pub(crate) fn append_shared_event(
        &self,
        record: Arc<OperationalJournalRecord>,
    ) -> RuntimeResult<()> {
        if !matches!(record.as_ref(), OperationalJournalRecord::Event(_)) {
            return Err(journal_message(
                "append_event",
                "shared operational record is not an event".to_string(),
            ));
        }
        self.append_shared(record)
    }

    pub(crate) fn append_shared_audit(
        &self,
        record: Arc<OperationalJournalRecord>,
    ) -> RuntimeResult<()> {
        match record.as_ref() {
            OperationalJournalRecord::Audit(entry) if entry.is_bounded_and_redacted() => {}
            OperationalJournalRecord::Audit(_) => {
                return Err(journal_message(
                    "append_audit",
                    "shared audit entry is not redacted and bounded".to_string(),
                ));
            }
            OperationalJournalRecord::Event(_) => {
                return Err(journal_message(
                    "append_audit",
                    "shared operational record is not an audit entry".to_string(),
                ));
            }
        }
        self.append_shared(record)
    }

    pub(crate) fn shared_audit_records(&self) -> Vec<Arc<OperationalJournalRecord>> {
        let records = Arc::clone(&self.state.lock().records);
        records
            .iter()
            .filter(|record| matches!(record.as_ref(), OperationalJournalRecord::Audit(_)))
            .cloned()
            .collect()
    }

    pub(crate) fn shared_event_records(&self) -> Vec<Arc<OperationalJournalRecord>> {
        let records = Arc::clone(&self.state.lock().records);
        records
            .iter()
            .filter(|record| matches!(record.as_ref(), OperationalJournalRecord::Event(_)))
            .cloned()
            .collect()
    }

    /// Returns retained audit entries in journal order.
    pub fn audit_entries(&self) -> Vec<AuditEntry> {
        let records = Arc::clone(&self.state.lock().records);
        records
            .iter()
            .filter_map(|record| match record.as_ref() {
                OperationalJournalRecord::Audit(entry) => Some(entry.clone()),
                OperationalJournalRecord::Event(_) => None,
            })
            .collect()
    }

    /// Returns retained event envelopes in journal order.
    pub fn events(&self) -> Vec<EventEnvelope> {
        let records = Arc::clone(&self.state.lock().records);
        records
            .iter()
            .filter_map(|record| match record.as_ref() {
                OperationalJournalRecord::Event(event) => Some(event.clone()),
                OperationalJournalRecord::Audit(_) => None,
            })
            .collect()
    }

    /// Exports retained audit entries as newline-delimited JSON.
    pub fn export_audit_jsonl(&self) -> RuntimeResult<String> {
        let mut output = Vec::new();
        self.write_audit_jsonl(&mut output)?;
        String::from_utf8(output)
            .map_err(|error| journal_message("serialize_export", error.to_string()))
    }

    /// Writes retained audit entries as newline-delimited JSON without cloning the records.
    pub fn write_audit_jsonl(&self, writer: &mut impl Write) -> RuntimeResult<()> {
        let records = Arc::clone(&self.state.lock().records);
        for record in records.iter() {
            let OperationalJournalRecord::Audit(entry) = record.as_ref() else {
                continue;
            };
            serde_json::to_writer(&mut *writer, entry)
                .map_err(|error| journal_message("serialize_export", error.to_string()))?;
            writer
                .write_all(b"\n")
                .map_err(|error| journal_io("write_export", error))?;
        }
        Ok(())
    }

    fn append(&self, record: OperationalJournalRecord) -> RuntimeResult<()> {
        self.append_shared(Arc::new(record))
    }

    fn append_shared(&self, record: Arc<OperationalJournalRecord>) -> RuntimeResult<()> {
        let mut state = self.state.lock();
        let sequence = state.sequence.saturating_add(1);
        let hash = record_hash(sequence, &state.last_hash, record.as_ref())?;
        append_envelope(
            &self.path,
            sequence,
            &state.last_hash,
            &hash,
            record.as_ref(),
        )?;
        Arc::make_mut(&mut state.records).push_back(record);
        state.sequence = sequence;
        state.last_hash = hash;
        if state.records.len() > self.max_records
            || fs::metadata(&self.path)
                .map(|metadata| metadata.len() > self.max_bytes)
                .unwrap_or(true)
        {
            self.compact_locked(&mut state)?;
        }
        Ok(())
    }

    fn compact_locked(&self, state: &mut JournalState) -> RuntimeResult<()> {
        let records = Arc::make_mut(&mut state.records);
        while records.len() > self.max_records {
            records.pop_front();
        }
        retain_within_bytes(records, self.max_bytes)?;
        self.rewrite_locked(state)
    }

    fn rewrite_locked(&self, state: &mut JournalState) -> RuntimeResult<()> {
        let (sequence, last_hash) = atomic_replace(&self.path, |file| {
            encode_records(file, state.records.iter().map(AsRef::as_ref))
        })?;
        state.sequence = sequence;
        state.last_hash = last_hash;
        Ok(())
    }
}

fn load_state(path: &Path, max_bytes: u64) -> RuntimeResult<(JournalState, bool)> {
    reject_symlink(path)?;
    let metadata = fs::metadata(path).map_err(|error| journal_io("read_metadata", error))?;
    if metadata.len() > max_bytes {
        return Err(journal_message(
            "validate_size",
            "journal exceeds size limit".to_string(),
        ));
    }
    let file = File::open(path).map_err(|error| journal_io("open_read", error))?;
    let mut reader = BufReader::new(file).take(max_bytes.saturating_add(1));
    let mut line = Vec::new();
    let header = read_bounded_line(&mut reader, &mut line, OPERATIONAL_JOURNAL_FORMAT_V1.len())?;
    if !matches!(header, LineStatus::Complete)
        || line.as_slice() != OPERATIONAL_JOURNAL_FORMAT_V1.as_bytes()
    {
        return Err(journal_message(
            "validate_format",
            "unsupported operational journal format".to_string(),
        ));
    }
    let mut records = VecDeque::new();
    let mut sequence = 0u64;
    let mut last_hash = String::new();
    let mut sanitized_record = false;
    let recovered_tail = loop {
        match read_bounded_line(&mut reader, &mut line, MAX_JOURNAL_ENVELOPE_BYTES)? {
            LineStatus::End => break false,
            LineStatus::Partial => break true,
            LineStatus::Complete if line.iter().all(u8::is_ascii_whitespace) => {}
            LineStatus::Complete => {
                let envelope: JournalEnvelope = serde_json::from_slice(&line)
                    .map_err(|error| journal_message("parse_record", error.to_string()))?;
                validate_envelope(&envelope, sequence.saturating_add(1), &last_hash)?;
                sequence = envelope.sequence;
                last_hash = envelope.hash;
                let (record, sanitized) = sanitize_loaded_record(envelope.record);
                sanitized_record |= sanitized;
                records.push_back(Arc::new(record));
            }
        }
    };
    if reader.limit() == 0 {
        return Err(journal_message(
            "validate_size",
            "journal exceeds size limit".to_string(),
        ));
    }
    Ok((
        JournalState {
            records: Arc::new(records),
            sequence,
            last_hash,
        },
        recovered_tail || sanitized_record,
    ))
}

fn sanitize_loaded_record(record: OperationalJournalRecord) -> (OperationalJournalRecord, bool) {
    match record {
        OperationalJournalRecord::Audit(entry) if !entry.is_bounded_and_redacted() => {
            (OperationalJournalRecord::Audit(entry.into_bounded()), true)
        }
        record => (record, false),
    }
}

fn validate_envelope(
    envelope: &JournalEnvelope,
    expected_sequence: u64,
    expected_previous: &str,
) -> RuntimeResult<()> {
    let expected_hash = record_hash(envelope.sequence, expected_previous, &envelope.record)?;
    if envelope.sequence != expected_sequence
        || envelope.previous_hash != expected_previous
        || envelope.hash != expected_hash
    {
        return Err(journal_message(
            "validate_hash_chain",
            "operational journal hash chain mismatch".to_string(),
        ));
    }
    Ok(())
}

fn retain_within_bytes(
    records: &mut VecDeque<Arc<OperationalJournalRecord>>,
    max_bytes: u64,
) -> RuntimeResult<()> {
    let retained = retained_suffix_count(records, max_bytes)?;
    if retained < records.len() {
        records.drain(..records.len() - retained);
    }
    Ok(())
}

fn read_bounded_line<R: BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
    max_bytes: usize,
) -> RuntimeResult<LineStatus> {
    line.clear();
    loop {
        let available = reader
            .fill_buf()
            .map_err(|error| journal_io("read", error))?;
        if available.is_empty() {
            return Ok(if line.is_empty() {
                LineStatus::End
            } else {
                LineStatus::Partial
            });
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let consumed = newline.map_or(available.len(), |index| index + 1);
        let body_bytes = newline.unwrap_or(available.len());
        if line.len().saturating_add(body_bytes) > max_bytes {
            return Err(journal_message(
                "validate_record",
                "record exceeds size limit".to_string(),
            ));
        }
        line.extend_from_slice(&available[..body_bytes]);
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(LineStatus::Complete);
        }
    }
}

fn atomic_replace<T>(
    path: &Path,
    write: impl FnOnce(&mut File) -> RuntimeResult<T>,
) -> RuntimeResult<T> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let temporary = parent.join(format!(
        ".operational-journal.{}-{}.tmp",
        std::process::id(),
        JOURNAL_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| journal_io("open_temporary", error))?;
        set_private_file(&file)?;
        let output = write(&mut file)?;
        file.sync_all()
            .map_err(|error| journal_io("write_temporary", error))?;
        fs::rename(&temporary, path).map_err(|error| journal_io("replace", error))?;
        sync_parent(parent)?;
        Ok(output)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn open_lock(path: &Path) -> RuntimeResult<File> {
    reject_symlink(path)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| journal_io("open_lock", error))?;
    set_private_file(&file)?;
    Ok(file)
}

fn reject_symlink(path: &Path) -> RuntimeResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
            journal_message("validate_path", "journal path is unsafe".to_string()),
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(journal_io("inspect_path", error)),
    }
}

#[cfg(unix)]
fn set_private_file(file: &File) -> RuntimeResult<()> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| journal_io("set_permissions", error))
}

#[cfg(not(unix))]
fn set_private_file(_file: &File) -> RuntimeResult<()> {
    Ok(())
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> RuntimeResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| journal_io("sync_parent", error))
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> RuntimeResult<()> {
    Ok(())
}

pub(super) fn journal_io(operation: &'static str, error: std::io::Error) -> RuntimeError {
    journal_message(operation, error.to_string())
}

pub(super) fn journal_message(operation: &'static str, message: String) -> RuntimeError {
    RuntimeError::OperationalJournalIo { operation, message }
}

#[cfg(test)]
#[path = "operational_journal_tests.rs"]
mod tests;
