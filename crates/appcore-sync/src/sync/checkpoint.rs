// =============================================================================
//        #######
//     ###       ###     F: checkpoint.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Per-peer checkpoint contracts and local implementations.

use crate::sync::error::SyncResult;
use crate::sync::persistence::{
    acquire_persistence_lock, atomic_write, read_bounded_text, split_format,
};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Stable on-disk format marker for peer checkpoints.
pub const SYNC_CHECKPOINT_FORMAT_V1: &str = "# appcore-sync-checkpoint-v1";
const MAX_CHECKPOINT_FILE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_CHECKPOINT_PEER_ID_BYTES: usize = 256;
type Checkpoint = (u64, String);
type CheckpointMap = BTreeMap<String, Checkpoint>;

/// Sync checkpoint storage contract by peer id.
pub trait SyncCheckpointStore: Send + Sync {
    /// Returns the last accepted sequence and batch hash for `peer_id`.
    fn get_checkpoint(&self, peer_id: &str) -> SyncResult<Option<(u64, String)>>;
    /// Atomically replaces the sequence and batch hash for `peer_id`.
    fn set_checkpoint(&self, peer_id: &str, sequence: u64, hash: &str) -> SyncResult<()>;

    /// Returns the last accepted sequence, or zero when no checkpoint exists.
    fn get_last_sequence(&self, peer_id: &str) -> SyncResult<u64> {
        Ok(self
            .get_checkpoint(peer_id)?
            .map(|(seq, _)| seq)
            .unwrap_or(0))
    }

    /// Updates only the accepted sequence while preserving the stored hash.
    fn set_last_sequence(&self, peer_id: &str, sequence: u64) -> SyncResult<()> {
        let hash = self
            .get_checkpoint(peer_id)?
            .map(|(_, h)| h)
            .unwrap_or_default();
        self.set_checkpoint(peer_id, sequence, &hash)
    }
}

/// In-memory checkpoint store for tests/local runtime.
#[derive(Debug, Clone, Default)]
pub struct InMemorySyncCheckpointStore {
    checkpoints: Arc<Mutex<CheckpointMap>>,
}

impl InMemorySyncCheckpointStore {
    /// Creates an empty process-local checkpoint store.
    pub fn new() -> Self {
        Self {
            checkpoints: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }
}

impl SyncCheckpointStore for InMemorySyncCheckpointStore {
    fn get_checkpoint(&self, peer_id: &str) -> SyncResult<Option<(u64, String)>> {
        validate_peer_id(peer_id)?;
        let guard = self.checkpoints.lock();
        Ok(guard.get(peer_id).cloned())
    }

    fn set_checkpoint(&self, peer_id: &str, sequence: u64, hash: &str) -> SyncResult<()> {
        validate_peer_id(peer_id)?;
        validate_checkpoint_hash(hash)?;
        let mut guard = self.checkpoints.lock();
        guard.insert(peer_id.to_string(), (sequence, hash.to_string()));
        Ok(())
    }
}

/// File-backed checkpoint store (line-based `peer=sequence,hash`).
#[derive(Debug, Clone)]
pub struct FileSyncCheckpointStore {
    file_path: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl FileSyncCheckpointStore {
    /// Opens or creates an atomic line-based checkpoint file.
    pub fn new(file_path: impl Into<PathBuf>) -> SyncResult<Self> {
        let file_path = file_path.into();
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|err| crate::sync::error::SyncError::ReplicationFailed(err.to_string()))?;
        }
        let _process_lock = acquire_persistence_lock(&file_path)?;
        if !file_path.exists() {
            atomic_write(
                &file_path,
                format!("{SYNC_CHECKPOINT_FORMAT_V1}\n").as_bytes(),
            )?;
        }
        let store = Self {
            file_path,
            lock: Arc::new(Mutex::new(())),
        };
        store.read_state()?;
        Ok(store)
    }

    /// Returns the durable checkpoint file path.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    fn read_state(&self) -> SyncResult<CheckpointMap> {
        let text = read_bounded_text(&self.file_path, MAX_CHECKPOINT_FILE_BYTES)?;
        let formatted = split_format(&text, SYNC_CHECKPOINT_FORMAT_V1)?;
        parse_checkpoint_map(formatted.body)
    }

    fn read_map(&self) -> SyncResult<CheckpointMap> {
        self.read_state()
    }

    fn write_map(&self, map: &CheckpointMap) -> SyncResult<()> {
        let mut out = format!("{SYNC_CHECKPOINT_FORMAT_V1}\n");
        for (peer_id, (sequence, hash)) in map {
            out.push_str(peer_id);
            out.push('=');
            out.push_str(&sequence.to_string());
            out.push(',');
            out.push_str(hash);
            out.push('\n');
        }
        atomic_write(&self.file_path, out.as_bytes())
    }
}

impl SyncCheckpointStore for FileSyncCheckpointStore {
    fn get_checkpoint(&self, peer_id: &str) -> SyncResult<Option<(u64, String)>> {
        validate_peer_id(peer_id)?;
        let _guard = self.lock.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        let map = self.read_map()?;
        Ok(map.get(peer_id).cloned())
    }

    fn set_checkpoint(&self, peer_id: &str, sequence: u64, hash: &str) -> SyncResult<()> {
        validate_peer_id(peer_id)?;
        validate_checkpoint_hash(hash)?;
        let _guard = self.lock.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        let mut map = self.read_map()?;
        map.insert(peer_id.to_string(), (sequence, hash.to_string()));
        self.write_map(&map)
    }
}

fn validate_peer_id(peer_id: &str) -> SyncResult<()> {
    if peer_id.is_empty() || peer_id.len() > MAX_CHECKPOINT_PEER_ID_BYTES {
        return Err(crate::sync::error::SyncError::InvalidPeerId);
    }
    if !peer_id
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | ':' | '-'))
    {
        return Err(crate::sync::error::SyncError::InvalidPeerId);
    }
    Ok(())
}

fn validate_checkpoint_hash(hash: &str) -> SyncResult<()> {
    if hash.is_empty() || (hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit())) {
        return Ok(());
    }
    Err(crate::sync::error::SyncError::ReplicationFailed(
        "invalid checkpoint hash".to_string(),
    ))
}

fn parse_checkpoint_map(text: &str) -> SyncResult<CheckpointMap> {
    let mut map = BTreeMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let (peer_id, rest) = line.split_once('=').ok_or_else(|| {
            crate::sync::error::SyncError::ReplicationFailed("invalid checkpoint line".to_string())
        })?;
        validate_peer_id(peer_id)?;

        let (sequence_text, hash) = if let Some((seq_t, h_t)) = rest.split_once(',') {
            (seq_t, h_t.to_string())
        } else {
            (rest, "".to_string())
        };

        let sequence = sequence_text.parse::<u64>().map_err(|_| {
            crate::sync::error::SyncError::ReplicationFailed(
                "invalid checkpoint sequence".to_string(),
            )
        })?;
        validate_checkpoint_hash(&hash)?;
        map.insert(peer_id.to_string(), (sequence, hash));
    }
    Ok(map)
}
