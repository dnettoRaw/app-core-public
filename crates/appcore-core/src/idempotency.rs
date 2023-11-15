// =============================================================================
//        #######
//     ###       ###     F: idempotency.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Idempotency stores used by runtime controller command deduplication.

use crate::error::{RuntimeError, RuntimeResult};
use crate::ids::validate_identifier;
use std::collections::HashMap;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable on-disk format marker for the file idempotency store.
pub const IDEMPOTENCY_FORMAT_V1: &str = "# appcore-idempotency-v1";
const MAX_IDEMPOTENCY_FILE_BYTES: u64 = 64 * 1024 * 1024;
// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static IDEMPOTENCY_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Status of an idempotency execution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub enum IdempotencyStatus {
    /// Command execution has been reserved but not completed.
    Pending,
    /// Command execution completed and its serialized response is reusable.
    Resolved {
        /// Stable response status.
        response_status: u16,
        /// Serialized response body.
        response_body: String,
    },
}

/// A stored idempotency execution record.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct IdempotencyRecord {
    /// Validated idempotency key.
    pub key: String,
    /// Digest that binds the key to one logical request.
    pub request_hash: String,
    /// Current execution status.
    pub status: IdempotencyStatus,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: u64,
}

/// Durable or process-local idempotency record boundary.
pub trait IdempotencyStore: Send + Sync {
    /// Returns a stored record by key.
    fn get(&self, key: &str) -> RuntimeResult<Option<IdempotencyRecord>>;
    /// Inserts or replaces a validated record.
    fn insert(&mut self, record: IdempotencyRecord) -> RuntimeResult<()>;
    /// Returns the number of active records.
    fn len(&self) -> usize;

    /// Removes a record when supported.
    fn remove(&mut self, _key: &str) -> RuntimeResult<()> {
        Ok(())
    }

    /// Reports whether no active records exist.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Process-local idempotency store.
#[derive(Default)]
pub struct InMemoryIdempotencyStore {
    seen: HashMap<String, IdempotencyRecord>,
}

impl fmt::Debug for InMemoryIdempotencyStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InMemoryIdempotencyStore")
            .field("entry_count", &self.seen.len())
            .finish()
    }
}

impl InMemoryIdempotencyStore {
    /// Creates an empty process-local store.
    pub fn new() -> Self {
        Self::default()
    }
}

impl IdempotencyStore for InMemoryIdempotencyStore {
    fn get(&self, key: &str) -> RuntimeResult<Option<IdempotencyRecord>> {
        Ok(self.seen.get(key).cloned())
    }

    fn insert(&mut self, record: IdempotencyRecord) -> RuntimeResult<()> {
        validate_key(&record.key)?;
        self.seen.insert(record.key.clone(), record);
        Ok(())
    }

    fn len(&self) -> usize {
        self.seen.len()
    }

    fn remove(&mut self, key: &str) -> RuntimeResult<()> {
        self.seen.remove(key);
        Ok(())
    }
}

/// Append-oriented local idempotency store with atomic compaction.
pub struct FileIdempotencyStore {
    file_path: PathBuf,
    ttl_ms: Option<u64>,
    seen: HashMap<String, IdempotencyRecord>,
}

impl fmt::Debug for FileIdempotencyStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileIdempotencyStore")
            .field("file_path", &self.file_path)
            .field("ttl_ms", &self.ttl_ms)
            .field("entry_count", &self.seen.len())
            .finish()
    }
}

fn map_idempotency_io(operation: &'static str, err: std::io::Error) -> RuntimeError {
    RuntimeError::IdempotencyStoreIo {
        operation,
        message: err.to_string(),
    }
}

impl FileIdempotencyStore {
    /// Opens a store without expiration.
    pub fn new(path: impl AsRef<Path>) -> RuntimeResult<Self> {
        Self::new_with_ttl(path, None)
    }

    /// Opens a store with optional record expiration.
    pub fn new_with_ttl(path: impl AsRef<Path>, ttl_ms: Option<u64>) -> RuntimeResult<Self> {
        let file_path = path.as_ref().to_path_buf();
        ensure_parent_dir(&file_path)?;
        if !file_path.exists() {
            rewrite_entries(&file_path, &HashMap::new())?;
        }
        let (seen, needs_rewrite) = load_entries(&file_path)?;
        if needs_rewrite {
            rewrite_entries(&file_path, &seen)?;
        }
        let ttl_ms = match ttl_ms {
            Some(0) => None,
            other => other,
        };

        Ok(Self {
            file_path,
            ttl_ms,
            seen,
        })
    }

    /// Returns the backing file path.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Removes expired records and atomically rewrites the backing file.
    pub fn compact(&mut self, now_ms: u64) -> RuntimeResult<usize> {
        let before = self.seen.len();
        self.seen
            .retain(|_, record| !is_expired(record.created_at_ms, self.ttl_ms, now_ms));
        let removed = before.saturating_sub(self.seen.len());

        rewrite_entries(&self.file_path, &self.seen)?;

        Ok(removed)
    }
}

impl IdempotencyStore for FileIdempotencyStore {
    fn get(&self, key: &str) -> RuntimeResult<Option<IdempotencyRecord>> {
        let now_ms = now_ms();
        if let Some(record) = self.seen.get(key) {
            if is_expired(record.created_at_ms, self.ttl_ms, now_ms) {
                Ok(None)
            } else {
                Ok(Some(record.clone()))
            }
        } else {
            Ok(None)
        }
    }

    fn insert(&mut self, record: IdempotencyRecord) -> RuntimeResult<()> {
        validate_key(&record.key)?;
        append_entry(&self.file_path, &record)?;
        self.seen.insert(record.key.clone(), record);
        Ok(())
    }

    fn len(&self) -> usize {
        let now_ms = now_ms();
        self.seen
            .values()
            .filter(|record| !is_expired(record.created_at_ms, self.ttl_ms, now_ms))
            .count()
    }

    fn remove(&mut self, key: &str) -> RuntimeResult<()> {
        if self.seen.remove(key).is_some() {
            rewrite_entries(&self.file_path, &self.seen)?;
        }
        Ok(())
    }
}

fn ensure_parent_dir(file_path: &Path) -> RuntimeResult<()> {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| map_idempotency_io("create_store_parent_dir", e))?;
    }
    Ok(())
}

fn load_entries(path: &Path) -> RuntimeResult<(HashMap<String, IdempotencyRecord>, bool)> {
    reject_symlink(path)?;
    let metadata =
        fs::metadata(path).map_err(|error| map_idempotency_io("read_store_metadata", error))?;
    if metadata.len() > MAX_IDEMPOTENCY_FILE_BYTES {
        return Err(corrupt_idempotency("store exceeds size limit"));
    }
    let text = fs::read_to_string(path).map_err(|error| map_idempotency_io("read_store", error))?;
    let body = split_idempotency_format(&text)?;
    let (complete, recovered_tail) = complete_line_prefix(body);
    let mut seen = HashMap::new();

    for line in complete.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record = parse_idempotency_record(trimmed)?;
        validate_key(&record.key)?;
        if matches!(record.status, IdempotencyStatus::Resolved { .. }) {
            seen.insert(record.key.clone(), record);
        }
    }
    Ok((seen, recovered_tail))
}

fn append_entry(path: &Path, record: &IdempotencyRecord) -> RuntimeResult<()> {
    let mut file = OpenOptions::new()
        .append(true)
        .open(path)
        .map_err(|e| map_idempotency_io("open_store_for_append", e))?;
    let line = serde_json::to_string(record).map_err(|e| RuntimeError::IdempotencyStoreIo {
        operation: "serialize_store_entry",
        message: e.to_string(),
    })?;
    writeln!(file, "{}", line).map_err(|e| map_idempotency_io("append_store_entry", e))?;
    file.sync_data()
        .map_err(|e| map_idempotency_io("sync_store_entry", e))?;
    Ok(())
}

fn rewrite_entries(path: &Path, entries: &HashMap<String, IdempotencyRecord>) -> RuntimeResult<()> {
    let mut rows: Vec<(&String, &IdempotencyRecord)> = entries.iter().collect();
    rows.sort_by(|a, b| a.0.cmp(b.0));

    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    reject_symlink(path)?;
    let temp_name = format!(
        ".{}.{}-{}.tmp",
        path.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("idempotency"),
        std::process::id(),
        IDEMPOTENCY_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let temp_path = parent.join(temp_name);

    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|e| map_idempotency_io("open_temp_store_for_rewrite", e))?;
        writeln!(file, "{IDEMPOTENCY_FORMAT_V1}")
            .map_err(|e| map_idempotency_io("write_store_format", e))?;
        for (_, record) in rows {
            let line =
                serde_json::to_string(record).map_err(|e| RuntimeError::IdempotencyStoreIo {
                    operation: "serialize_store_entry",
                    message: e.to_string(),
                })?;
            writeln!(file, "{}", line).map_err(|e| map_idempotency_io("rewrite_store_entry", e))?;
        }
        file.sync_all()
            .map_err(|e| map_idempotency_io("sync_temp_store", e))?;
        fs::rename(&temp_path, path).map_err(|e| map_idempotency_io("rename_temp_store", e))?;
        sync_parent_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp_path);
    }
    result
}

fn split_idempotency_format(text: &str) -> RuntimeResult<&str> {
    if let Some(body) = text
        .strip_prefix(IDEMPOTENCY_FORMAT_V1)
        .and_then(|rest| rest.strip_prefix('\n'))
    {
        return Ok(body);
    }
    if text == IDEMPOTENCY_FORMAT_V1 {
        return Ok("");
    }
    if text.starts_with("# appcore-") {
        return Err(corrupt_idempotency("NO MORE SUPPORTED PLEASE UPDATE"));
    }
    Err(corrupt_idempotency("NO MORE SUPPORTED PLEASE UPDATE"))
}

fn complete_line_prefix(body: &str) -> (&str, bool) {
    if body.is_empty() || body.ends_with('\n') {
        return (body, false);
    }
    match body.rfind('\n') {
        Some(last_newline) => (&body[..=last_newline], true),
        None => ("", true),
    }
}

fn parse_idempotency_record(line: &str) -> RuntimeResult<IdempotencyRecord> {
    if !line.starts_with('{') {
        return Err(corrupt_idempotency("NO MORE SUPPORTED PLEASE UPDATE"));
    }
    serde_json::from_str(line).map_err(|_| corrupt_idempotency("invalid JSON record"))
}

fn reject_symlink(path: &Path) -> RuntimeResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            Err(corrupt_idempotency("store path is not a regular file"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(map_idempotency_io("inspect_store_path", error)),
    }
}

fn corrupt_idempotency(message: &str) -> RuntimeError {
    RuntimeError::IdempotencyStoreIo {
        operation: "validate_store",
        message: message.to_string(),
    }
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> RuntimeResult<()> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| map_idempotency_io("sync_store_parent", error))
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> RuntimeResult<()> {
    Ok(())
}

fn validate_key(key: &str) -> RuntimeResult<()> {
    match validate_identifier("IdempotencyKey", key) {
        Ok(()) => Ok(()),
        Err(RuntimeError::InvalidIdentifier {
            reason: "empty", ..
        }) => Err(RuntimeError::InvalidIdempotencyKey { reason: "empty" }),
        Err(_) => Err(RuntimeError::InvalidIdempotencyKey {
            reason: "invalid_char",
        }),
    }
}

fn is_expired(created_at_ms: u64, ttl_ms: Option<u64>, now_ms: u64) -> bool {
    if created_at_ms == 0 {
        return false;
    }
    match ttl_ms {
        Some(ttl) => now_ms.saturating_sub(created_at_ms) > ttl,
        None => false,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "idempotency_tests.rs"]
mod tests;
