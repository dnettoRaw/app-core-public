// =============================================================================
//        #######
//     ###       ###     F: idempotency_file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounded incremental persistence and compact indexing for idempotency.

use crate::error::{RuntimeError, RuntimeResult};
use crate::idempotency::{IdempotencyRecord, IdempotencyStatus, IDEMPOTENCY_FORMAT_V1};
use crate::idempotency_encoding::{measure_record, RecordEncoding};
use crate::ids::validate_identifier;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

pub(crate) const MAX_IDEMPOTENCY_FILE_BYTES: u64 = 64 * 1024 * 1024;
pub(crate) use crate::idempotency_encoding::MAX_IDEMPOTENCY_RECORD_BYTES;
pub(crate) const MAX_ACTIVE_IDEMPOTENCY_RECORDS: usize = 65_536;
pub(crate) const MAX_PERSISTED_IDEMPOTENCY_RECORDS: usize = 131_072;
// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static IDEMPOTENCY_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy)]
pub(crate) struct StoreFileState {
    pub(crate) bytes: u64,
    pub(crate) records: usize,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RecordLocation {
    pub(crate) offset: u64,
    pub(crate) encoded_bytes: u32,
    pub(crate) created_at_ms: u64,
    digest: [u8; 32],
}

pub(crate) struct LoadedEntries {
    pub(crate) entries: HashMap<String, RecordLocation>,
    pub(crate) file_state: StoreFileState,
    pub(crate) needs_rewrite: bool,
}

pub(crate) struct RewrittenEntries {
    pub(crate) entries: HashMap<String, RecordLocation>,
    pub(crate) file_state: StoreFileState,
}

pub(crate) struct AppendedEntry {
    pub(crate) location: RecordLocation,
    pub(crate) written_bytes: u64,
}

enum LineStatus {
    Complete,
    Partial,
    End,
}

pub(crate) fn load_entries(path: &Path) -> RuntimeResult<LoadedEntries> {
    reject_symlink(path)?;
    let metadata = fs::metadata(path).map_err(|error| map_io("read_store_metadata", error))?;
    if metadata.len() > MAX_IDEMPOTENCY_FILE_BYTES {
        return Err(corrupt("store exceeds size limit"));
    }
    let file = File::open(path).map_err(|error| map_io("read_store", error))?;
    let mut reader = BufReader::new(file).take(MAX_IDEMPOTENCY_FILE_BYTES + 1);
    let mut line = Vec::new();
    let header = read_bounded_line(&mut reader, &mut line, IDEMPOTENCY_FORMAT_V1.len())?;
    validate_header(&line, &header)?;
    let mut needs_rewrite = matches!(header, LineStatus::Partial);
    let mut entries = HashMap::new();
    let mut records = 0usize;

    if !needs_rewrite {
        loop {
            let offset = consumed_bytes(&reader);
            match read_bounded_line(&mut reader, &mut line, MAX_IDEMPOTENCY_RECORD_BYTES)? {
                LineStatus::End => break,
                LineStatus::Partial => {
                    needs_rewrite = true;
                    break;
                }
                LineStatus::Complete => {
                    index_complete_line(&line, offset, &mut entries, &mut records)?;
                }
            }
        }
    }
    if reader.limit() == 0 {
        return Err(corrupt("store exceeds size limit"));
    }
    Ok(LoadedEntries {
        entries,
        file_state: StoreFileState {
            bytes: consumed_bytes(&reader),
            records,
        },
        needs_rewrite,
    })
}

pub(crate) fn read_entry(
    path: &Path,
    expected_key: &str,
    location: &RecordLocation,
) -> RuntimeResult<IdempotencyRecord> {
    reject_symlink(path)?;
    let mut file = File::open(path).map_err(|error| map_io("read_store_entry", error))?;
    read_entry_from(&mut file, expected_key, location)
}

pub(crate) fn encoded_record_bytes(record: &IdempotencyRecord) -> RuntimeResult<u64> {
    measure_record(record).map(|encoding| encoding.bytes)
}

pub(crate) fn append_entry(
    path: &Path,
    record: &IdempotencyRecord,
    expected_offset: u64,
) -> RuntimeResult<AppendedEntry> {
    reject_symlink(path)?;
    let encoding = measure_record(record)?;
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|error| map_io("open_store_for_append", error))?;
    if file
        .metadata()
        .map_err(|error| map_io("read_store_metadata", error))?
        .len()
        != expected_offset
    {
        return Err(corrupt("store changed outside its owner"));
    }
    write_record(&mut file, record, "append_store_entry")?;
    file.sync_data()
        .map_err(|error| map_io("sync_store_entry", error))?;
    Ok(AppendedEntry {
        location: location(expected_offset, &encoding, record.created_at_ms)?,
        written_bytes: encoding.bytes + 1,
    })
}

pub(crate) fn rewrite_entries<F>(
    path: &Path,
    entries: &HashMap<String, RecordLocation>,
    replacement: Option<&IdempotencyRecord>,
    keep: F,
) -> RuntimeResult<RewrittenEntries>
where
    F: Fn(&str, &RecordLocation) -> bool,
{
    let mut keys: Vec<&str> = entries
        .iter()
        .filter(|(key, item)| keep(key, item))
        .map(|(key, _)| key.as_str())
        .collect();
    if let Some(record) = replacement {
        if !keys.contains(&record.key.as_str()) {
            keys.push(record.key.as_str());
        }
    }
    if keys.len() > MAX_ACTIVE_IDEMPOTENCY_RECORDS {
        return Err(corrupt("active record limit exceeded"));
    }
    keys.sort_unstable();
    rewrite_selected(path, entries, replacement, keys)
}

fn rewrite_selected(
    path: &Path,
    entries: &HashMap<String, RecordLocation>,
    replacement: Option<&IdempotencyRecord>,
    keys: Vec<&str>,
) -> RuntimeResult<RewrittenEntries> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    reject_symlink(path)?;
    let source = if entries.is_empty() {
        None
    } else {
        Some(File::open(path).map_err(|error| map_io("read_store", error))?)
    };
    let temp_path = parent.join(temp_name(path));
    let result = write_replacement(&temp_path, path, entries, replacement, keys, source);
    if result.is_err() {
        let _ = fs::remove_file(temp_path);
    }
    result
}

fn write_replacement(
    temp_path: &Path,
    path: &Path,
    entries: &HashMap<String, RecordLocation>,
    replacement: Option<&IdempotencyRecord>,
    keys: Vec<&str>,
    mut source: Option<File>,
) -> RuntimeResult<RewrittenEntries> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let mut target = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(temp_path)
        .map_err(|error| map_io("open_temp_store_for_rewrite", error))?;
    target
        .write_all(IDEMPOTENCY_FORMAT_V1.as_bytes())
        .and_then(|()| target.write_all(b"\n"))
        .map_err(|error| map_io("write_store_format", error))?;
    let mut bytes = (IDEMPOTENCY_FORMAT_V1.len() + 1) as u64;
    let mut rewritten = HashMap::with_capacity(keys.len());
    for key in keys {
        let loaded;
        let record = match replacement {
            Some(item) if item.key == key => item,
            _ => {
                let old_location = entries
                    .get(key)
                    .ok_or_else(|| corrupt("active record changed during rewrite"))?;
                loaded = read_entry_from(
                    source
                        .as_mut()
                        .ok_or_else(|| corrupt("missing source during rewrite"))?,
                    key,
                    old_location,
                )?;
                &loaded
            }
        };
        let encoding = measure_record(record)?;
        let next = bytes
            .checked_add(encoding.bytes + 1)
            .filter(|value| *value <= MAX_IDEMPOTENCY_FILE_BYTES)
            .ok_or_else(|| corrupt("active records exceed store size limit"))?;
        write_record(&mut target, record, "rewrite_store_entry")?;
        rewritten.insert(
            key.to_string(),
            location(bytes, &encoding, record.created_at_ms)?,
        );
        bytes = next;
    }
    target
        .sync_all()
        .map_err(|error| map_io("sync_temp_store", error))?;
    drop(source);
    fs::rename(temp_path, path).map_err(|error| map_io("rename_temp_store", error))?;
    sync_parent_directory(parent)?;
    Ok(RewrittenEntries {
        file_state: StoreFileState {
            bytes,
            records: rewritten.len(),
        },
        entries: rewritten,
    })
}

fn read_entry_from(
    file: &mut File,
    expected_key: &str,
    location: &RecordLocation,
) -> RuntimeResult<IdempotencyRecord> {
    let length = location.encoded_bytes as usize;
    if length > MAX_IDEMPOTENCY_RECORD_BYTES {
        return Err(corrupt("indexed record exceeds size limit"));
    }
    file.seek(SeekFrom::Start(location.offset))
        .map_err(|error| map_io("seek_store_entry", error))?;
    let mut encoded = vec![0u8; length];
    file.read_exact(&mut encoded)
        .map_err(|error| map_io("read_store_entry", error))?;
    if Sha256::digest(&encoded).as_slice() != location.digest {
        return Err(corrupt("indexed record digest mismatch"));
    }
    let text = std::str::from_utf8(&encoded).map_err(|_| corrupt("invalid UTF-8 record"))?;
    let record: IdempotencyRecord =
        serde_json::from_str(text.trim()).map_err(|_| corrupt("invalid JSON record"))?;
    validate_record_key(&record.key)?;
    if record.key != expected_key || record.created_at_ms != location.created_at_ms {
        return Err(corrupt("indexed record identity mismatch"));
    }
    Ok(record)
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
            .map_err(|error| map_io("read_store", error))?;
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
            return Err(corrupt("record exceeds size limit"));
        }
        line.extend_from_slice(&available[..body_bytes]);
        reader.consume(consumed);
        if newline.is_some() {
            return Ok(LineStatus::Complete);
        }
    }
}

fn index_complete_line(
    line: &[u8],
    offset: u64,
    entries: &mut HashMap<String, RecordLocation>,
    records: &mut usize,
) -> RuntimeResult<()> {
    let text = std::str::from_utf8(line).map_err(|_| corrupt("invalid UTF-8 record"))?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    *records = records
        .checked_add(1)
        .filter(|count| *count <= MAX_PERSISTED_IDEMPOTENCY_RECORDS)
        .ok_or_else(|| corrupt("persisted record limit exceeded"))?;
    if !trimmed.starts_with('{') {
        return Err(corrupt("NO MORE SUPPORTED PLEASE UPDATE"));
    }
    let record: IdempotencyRecord =
        serde_json::from_str(trimmed).map_err(|_| corrupt("invalid JSON record"))?;
    validate_record_key(&record.key)?;
    if matches!(record.status, IdempotencyStatus::Resolved { .. }) {
        if !entries.contains_key(&record.key) && entries.len() >= MAX_ACTIVE_IDEMPOTENCY_RECORDS {
            return Err(corrupt("active record limit exceeded"));
        }
        entries.insert(
            record.key,
            RecordLocation {
                offset,
                encoded_bytes: u32::try_from(line.len())
                    .map_err(|_| corrupt("indexed record length overflow"))?,
                created_at_ms: record.created_at_ms,
                digest: Sha256::digest(line).into(),
            },
        );
    }
    Ok(())
}

fn location(
    offset: u64,
    encoding: &RecordEncoding,
    created_at_ms: u64,
) -> RuntimeResult<RecordLocation> {
    let encoded_bytes =
        u32::try_from(encoding.bytes).map_err(|_| corrupt("indexed record length overflow"))?;
    Ok(RecordLocation {
        offset,
        encoded_bytes,
        created_at_ms,
        digest: encoding.digest,
    })
}

fn consumed_bytes<R: BufRead>(reader: &std::io::Take<R>) -> u64 {
    MAX_IDEMPOTENCY_FILE_BYTES + 1 - reader.limit()
}

fn validate_header(line: &[u8], status: &LineStatus) -> RuntimeResult<()> {
    if matches!(status, LineStatus::End) || line != IDEMPOTENCY_FORMAT_V1.as_bytes() {
        return Err(corrupt("NO MORE SUPPORTED PLEASE UPDATE"));
    }
    Ok(())
}

fn write_record(
    writer: &mut impl Write,
    record: &IdempotencyRecord,
    operation: &'static str,
) -> RuntimeResult<()> {
    serde_json::to_writer(&mut *writer, record).map_err(serialize_error)?;
    writer
        .write_all(b"\n")
        .map_err(|error| map_io(operation, error))
}

fn validate_record_key(key: &str) -> RuntimeResult<()> {
    validate_identifier("IdempotencyKey", key)
        .map_err(|_| corrupt("invalid idempotency key in store"))
}

fn temp_name(path: &Path) -> String {
    format!(
        ".{}.{}-{}.tmp",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("idempotency"),
        std::process::id(),
        IDEMPOTENCY_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

fn reject_symlink(path: &Path) -> RuntimeResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(corrupt("store path is not a regular file"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(map_io("inspect_store_path", error)),
    }
}

fn serialize_error(error: serde_json::Error) -> RuntimeError {
    RuntimeError::IdempotencyStoreIo {
        operation: "serialize_store_entry",
        message: error.to_string(),
    }
}

fn map_io(operation: &'static str, error: std::io::Error) -> RuntimeError {
    RuntimeError::IdempotencyStoreIo {
        operation,
        message: error.to_string(),
    }
}

fn corrupt(message: &str) -> RuntimeError {
    RuntimeError::IdempotencyStoreIo {
        operation: "validate_store",
        message: message.to_string(),
    }
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> RuntimeResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| map_io("sync_store_parent", error))
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> RuntimeResult<()> {
    Ok(())
}
