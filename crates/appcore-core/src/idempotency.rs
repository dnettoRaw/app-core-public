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
use crate::idempotency_file::{
    append_entry, encoded_record_bytes, load_entries, read_entry, rewrite_entries, RecordLocation,
    StoreFileState, MAX_ACTIVE_IDEMPOTENCY_RECORDS, MAX_IDEMPOTENCY_FILE_BYTES,
    MAX_PERSISTED_IDEMPOTENCY_RECORDS,
};
use crate::ids::validate_identifier;
use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable on-disk format marker for the file idempotency store.
pub const IDEMPOTENCY_FORMAT_V1: &str = "# appcore-idempotency-v1";

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
        ensure_active_capacity(&self.seen, &record.key)?;
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
    seen: HashMap<String, RecordLocation>,
    file_state: StoreFileState,
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
            rewrite_entries(&file_path, &HashMap::new(), None, |_, _| true)?;
        }
        let loaded = load_entries(&file_path)?;
        let (seen, file_state) = if loaded.needs_rewrite {
            let rewritten = rewrite_entries(&file_path, &loaded.entries, None, |_, _| true)?;
            (rewritten.entries, rewritten.file_state)
        } else {
            (loaded.entries, loaded.file_state)
        };
        let ttl_ms = match ttl_ms {
            Some(0) => None,
            other => other,
        };

        Ok(Self {
            file_path,
            ttl_ms,
            seen,
            file_state,
        })
    }

    /// Returns the backing file path.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    /// Removes expired records and atomically rewrites the backing file.
    pub fn compact(&mut self, now_ms: u64) -> RuntimeResult<usize> {
        let before = self.seen.len();
        let ttl_ms = self.ttl_ms;
        let rewritten = rewrite_entries(&self.file_path, &self.seen, None, |_, location| {
            !is_expired(location.created_at_ms, ttl_ms, now_ms)
        })?;
        let removed = before.saturating_sub(rewritten.entries.len());
        self.seen = rewritten.entries;
        self.file_state = rewritten.file_state;

        Ok(removed)
    }
}

impl IdempotencyStore for FileIdempotencyStore {
    fn get(&self, key: &str) -> RuntimeResult<Option<IdempotencyRecord>> {
        let now_ms = now_ms();
        if let Some(location) = self.seen.get(key) {
            if is_expired(location.created_at_ms, self.ttl_ms, now_ms) {
                Ok(None)
            } else {
                read_entry(&self.file_path, key, location).map(Some)
            }
        } else {
            Ok(None)
        }
    }

    fn insert(&mut self, record: IdempotencyRecord) -> RuntimeResult<()> {
        validate_key(&record.key)?;
        ensure_active_capacity(&self.seen, &record.key)?;
        let record_bytes = encoded_record_bytes(&record)?;
        if self.requires_rewrite(record_bytes) {
            return self.replace_and_rewrite(record);
        }
        let appended = append_entry(&self.file_path, &record, self.file_state.bytes)?;
        self.file_state.bytes = self.file_state.bytes.saturating_add(appended.written_bytes);
        self.file_state.records = self.file_state.records.saturating_add(1);
        self.seen.insert(record.key, appended.location);
        Ok(())
    }

    fn len(&self) -> usize {
        let now_ms = now_ms();
        self.seen
            .values()
            .filter(|location| !is_expired(location.created_at_ms, self.ttl_ms, now_ms))
            .count()
    }

    fn remove(&mut self, key: &str) -> RuntimeResult<()> {
        if self.seen.contains_key(key) {
            let rewritten = rewrite_entries(&self.file_path, &self.seen, None, |candidate, _| {
                candidate != key
            })?;
            self.seen = rewritten.entries;
            self.file_state = rewritten.file_state;
        }
        Ok(())
    }
}

impl FileIdempotencyStore {
    fn requires_rewrite(&self, record_bytes: u64) -> bool {
        self.file_state.records >= MAX_PERSISTED_IDEMPOTENCY_RECORDS
            || self
                .file_state
                .bytes
                .checked_add(record_bytes.saturating_add(1))
                .is_none_or(|bytes| bytes > MAX_IDEMPOTENCY_FILE_BYTES)
    }

    fn replace_and_rewrite(&mut self, record: IdempotencyRecord) -> RuntimeResult<()> {
        let rewritten = rewrite_entries(&self.file_path, &self.seen, Some(&record), |_, _| true)?;
        self.seen = rewritten.entries;
        self.file_state = rewritten.file_state;
        Ok(())
    }
}

fn ensure_parent_dir(file_path: &Path) -> RuntimeResult<()> {
    if let Some(parent) = file_path.parent() {
        fs::create_dir_all(parent).map_err(|e| map_idempotency_io("create_store_parent_dir", e))?;
    }
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

fn ensure_active_capacity<T>(entries: &HashMap<String, T>, key: &str) -> RuntimeResult<()> {
    if !entries.contains_key(key) && entries.len() >= MAX_ACTIVE_IDEMPOTENCY_RECORDS {
        return Err(RuntimeError::IdempotencyStoreIo {
            operation: "validate_store",
            message: "active record limit exceeded".to_string(),
        });
    }
    Ok(())
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
