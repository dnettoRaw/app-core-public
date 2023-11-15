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

use crate::{AuditEntry, EventEnvelope, RuntimeError, RuntimeResult};
use fs2::FileExt;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Stable format marker for the operational journal.
pub const OPERATIONAL_JOURNAL_FORMAT_V1: &str = "# appcore-operational-journal-v1";
const MAX_JOURNAL_RECORD_BYTES: usize = 1024 * 1024;
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

struct JournalState {
    records: VecDeque<OperationalJournalRecord>,
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
    /// Opens a journal and validates its complete hash chain.
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
            atomic_write(
                &path,
                format!("{OPERATIONAL_JOURNAL_FORMAT_V1}\n").as_bytes(),
            )?;
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
        self.append(OperationalJournalRecord::Audit(entry))
    }

    /// Appends one opaque event envelope.
    pub fn append_event(&self, event: EventEnvelope) -> RuntimeResult<()> {
        self.append(OperationalJournalRecord::Event(event))
    }

    /// Returns retained audit entries in journal order.
    pub fn audit_entries(&self) -> Vec<AuditEntry> {
        self.state
            .lock()
            .records
            .iter()
            .filter_map(|record| match record {
                OperationalJournalRecord::Audit(entry) => Some(entry.clone()),
                OperationalJournalRecord::Event(_) => None,
            })
            .collect()
    }

    /// Returns retained event envelopes in journal order.
    pub fn events(&self) -> Vec<EventEnvelope> {
        self.state
            .lock()
            .records
            .iter()
            .filter_map(|record| match record {
                OperationalJournalRecord::Event(event) => Some(event.clone()),
                OperationalJournalRecord::Audit(_) => None,
            })
            .collect()
    }

    /// Exports retained audit entries as newline-delimited JSON.
    pub fn export_audit_jsonl(&self) -> RuntimeResult<String> {
        let mut output = String::new();
        for entry in self.audit_entries() {
            output.push_str(
                &serde_json::to_string(&entry)
                    .map_err(|error| journal_message("serialize_export", error.to_string()))?,
            );
            output.push('\n');
        }
        Ok(output)
    }

    fn append(&self, record: OperationalJournalRecord) -> RuntimeResult<()> {
        let record_bytes = serde_json::to_vec(&record)
            .map_err(|error| journal_message("serialize_record", error.to_string()))?;
        if record_bytes.len() > MAX_JOURNAL_RECORD_BYTES {
            return Err(journal_message(
                "validate_record",
                "record exceeds size limit".to_string(),
            ));
        }
        let mut state = self.state.lock();
        let sequence = state.sequence.saturating_add(1);
        let hash = record_hash(sequence, &state.last_hash, &record_bytes);
        let envelope = JournalEnvelope {
            sequence,
            previous_hash: state.last_hash.clone(),
            hash: hash.clone(),
            record: record.clone(),
        };
        append_envelope(&self.path, &envelope)?;
        state.records.push_back(record);
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
        while state.records.len() > self.max_records {
            state.records.pop_front();
        }
        self.rewrite_locked(state)?;
        while fs::metadata(&self.path)
            .map(|metadata| metadata.len() > self.max_bytes)
            .unwrap_or(false)
            && state.records.len() > 1
        {
            state.records.pop_front();
            self.rewrite_locked(state)?;
        }
        Ok(())
    }

    fn rewrite_locked(&self, state: &mut JournalState) -> RuntimeResult<()> {
        let (bytes, sequence, last_hash) = encode_records(&state.records)?;
        atomic_write(&self.path, &bytes)?;
        state.sequence = sequence;
        state.last_hash = last_hash;
        Ok(())
    }
}

fn load_state(path: &Path, max_bytes: u64) -> RuntimeResult<(JournalState, bool)> {
    let text = read_bounded(path, max_bytes)?;
    let body = text
        .strip_prefix(OPERATIONAL_JOURNAL_FORMAT_V1)
        .and_then(|rest| rest.strip_prefix('\n'))
        .ok_or_else(|| {
            journal_message(
                "validate_format",
                "unsupported operational journal format".to_string(),
            )
        })?;
    let (complete, recovered_tail) = complete_line_prefix(body);
    let mut records = VecDeque::new();
    let mut sequence = 0u64;
    let mut last_hash = String::new();
    for line in complete.lines().filter(|line| !line.is_empty()) {
        let envelope: JournalEnvelope = serde_json::from_str(line)
            .map_err(|error| journal_message("parse_record", error.to_string()))?;
        validate_envelope(&envelope, sequence.saturating_add(1), &last_hash)?;
        sequence = envelope.sequence;
        last_hash = envelope.hash;
        records.push_back(envelope.record);
    }
    Ok((
        JournalState {
            records,
            sequence,
            last_hash,
        },
        recovered_tail,
    ))
}

fn validate_envelope(
    envelope: &JournalEnvelope,
    expected_sequence: u64,
    expected_previous: &str,
) -> RuntimeResult<()> {
    let record = serde_json::to_vec(&envelope.record)
        .map_err(|error| journal_message("serialize_record", error.to_string()))?;
    let expected_hash = record_hash(envelope.sequence, expected_previous, &record);
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

fn encode_records(
    records: &VecDeque<OperationalJournalRecord>,
) -> RuntimeResult<(Vec<u8>, u64, String)> {
    let mut output = format!("{OPERATIONAL_JOURNAL_FORMAT_V1}\n");
    let mut sequence = 0u64;
    let mut last_hash = String::new();
    for record in records {
        sequence = sequence.saturating_add(1);
        let bytes = serde_json::to_vec(record)
            .map_err(|error| journal_message("serialize_record", error.to_string()))?;
        let hash = record_hash(sequence, &last_hash, &bytes);
        let envelope = JournalEnvelope {
            sequence,
            previous_hash: last_hash,
            hash: hash.clone(),
            record: record.clone(),
        };
        output.push_str(
            &serde_json::to_string(&envelope)
                .map_err(|error| journal_message("serialize_envelope", error.to_string()))?,
        );
        output.push('\n');
        last_hash = hash;
    }
    Ok((output.into_bytes(), sequence, last_hash))
}

fn append_envelope(path: &Path, envelope: &JournalEnvelope) -> RuntimeResult<()> {
    let line = serde_json::to_string(envelope)
        .map_err(|error| journal_message("serialize_envelope", error.to_string()))?;
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| journal_io("open_append", error))?;
    writeln!(file, "{line}").map_err(|error| journal_io("append_record", error))?;
    file.sync_data()
        .map_err(|error| journal_io("sync_record", error))
}

fn record_hash(sequence: u64, previous_hash: &str, record: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(OPERATIONAL_JOURNAL_FORMAT_V1.as_bytes());
    hasher.update(sequence.to_be_bytes());
    hasher.update((previous_hash.len() as u64).to_be_bytes());
    hasher.update(previous_hash.as_bytes());
    hasher.update((record.len() as u64).to_be_bytes());
    hasher.update(record);
    format!("{:x}", hasher.finalize())
}

fn complete_line_prefix(body: &str) -> (&str, bool) {
    if body.is_empty() || body.ends_with('\n') {
        return (body, false);
    }
    match body.rfind('\n') {
        Some(index) => (&body[..=index], true),
        None => ("", true),
    }
}

fn read_bounded(path: &Path, max_bytes: u64) -> RuntimeResult<String> {
    reject_symlink(path)?;
    let mut file = File::open(path).map_err(|error| journal_io("open_read", error))?;
    if file
        .metadata()
        .map_err(|error| journal_io("read_metadata", error))?
        .len()
        > max_bytes
    {
        return Err(journal_message(
            "validate_size",
            "journal exceeds size limit".to_string(),
        ));
    }
    let mut text = String::new();
    Read::by_ref(&mut file)
        .take(max_bytes.saturating_add(1))
        .read_to_string(&mut text)
        .map_err(|error| journal_io("read", error))?;
    Ok(text)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> RuntimeResult<()> {
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
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| journal_io("write_temporary", error))?;
        fs::rename(&temporary, path).map_err(|error| journal_io("replace", error))?;
        sync_parent(parent)
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

fn journal_io(operation: &'static str, error: std::io::Error) -> RuntimeError {
    journal_message(operation, error.to_string())
}

fn journal_message(operation: &'static str, message: String) -> RuntimeError {
    RuntimeError::OperationalJournalIo { operation, message }
}

#[cfg(test)]
#[path = "operational_journal_tests.rs"]
mod tests;
