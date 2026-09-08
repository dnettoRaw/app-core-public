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

use crate::sync::error::{SyncError, SyncResult, UPDATE_REQUIRED_MESSAGE};
use crate::sync::persistence::{
    acquire_persistence_lock, atomic_write, atomic_write_with, reject_symlink,
};
use parking_lot::Mutex;
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Stable on-disk format marker for peer checkpoints.
pub const SYNC_CHECKPOINT_FORMAT_V1: &str = "# appcore-sync-checkpoint-v1";
/// Maximum bytes accepted in one checkpoint file.
pub const MAX_CHECKPOINT_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Maximum UTF-8 bytes accepted in one checkpoint peer identifier.
pub const MAX_CHECKPOINT_PEER_ID_BYTES: usize = 256;
/// Maximum non-empty records accepted in one checkpoint file.
pub const MAX_CHECKPOINT_RECORDS: usize = 65_536;
const MAX_CHECKPOINT_LINE_BYTES: usize = MAX_CHECKPOINT_PEER_ID_BYTES + 87;
const CHECKPOINT_BUFFER_BYTES: usize = 16 * 1024;
type Checkpoint = (u64, String);
type CheckpointMap = BTreeMap<String, Checkpoint>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum LineStatus {
    Complete,
    Partial,
    End,
}

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
        store.validate_state()?;
        Ok(store)
    }

    /// Returns the durable checkpoint file path.
    pub fn file_path(&self) -> &Path {
        &self.file_path
    }

    fn validate_state(&self) -> SyncResult<()> {
        scan_checkpoint_file(&self.file_path, |_, _, _| Ok(()))
    }

    fn read_map(&self) -> SyncResult<CheckpointMap> {
        let mut map = BTreeMap::new();
        scan_checkpoint_file(&self.file_path, |peer_id, sequence, hash| {
            map.insert(peer_id.to_string(), (sequence, hash.to_string()));
            Ok(())
        })?;
        Ok(map)
    }

    fn write_map(&self, map: &CheckpointMap) -> SyncResult<()> {
        if map.len() > MAX_CHECKPOINT_RECORDS {
            return Err(checkpoint_failure("checkpoint record limit exceeded"));
        }
        let encoded_bytes = encoded_checkpoint_bytes(map)?;
        if encoded_bytes > MAX_CHECKPOINT_FILE_BYTES {
            return Err(checkpoint_failure(
                "persistent file exceeds configured limit",
            ));
        }
        atomic_write_with(&self.file_path, |file| write_checkpoint_map(file, map))
    }
}

impl SyncCheckpointStore for FileSyncCheckpointStore {
    fn get_checkpoint(&self, peer_id: &str) -> SyncResult<Option<(u64, String)>> {
        validate_peer_id(peer_id)?;
        let _guard = self.lock.lock();
        let _process_lock = acquire_persistence_lock(&self.file_path)?;
        let mut checkpoint = None;
        scan_checkpoint_file(&self.file_path, |stored_peer, sequence, hash| {
            if stored_peer == peer_id {
                checkpoint = Some((sequence, hash.to_string()));
            }
            Ok(())
        })?;
        Ok(checkpoint)
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

fn scan_checkpoint_file(
    path: &Path,
    mut visit: impl FnMut(&str, u64, &str) -> SyncResult<()>,
) -> SyncResult<()> {
    reject_symlink(path)?;
    let file = File::open(path).map_err(checkpoint_io)?;
    if file.metadata().map_err(checkpoint_io)?.len() > MAX_CHECKPOINT_FILE_BYTES {
        return Err(checkpoint_failure(
            "persistent file exceeds configured limit",
        ));
    }
    let limited = file.take(MAX_CHECKPOINT_FILE_BYTES.saturating_add(1));
    let mut reader = BufReader::with_capacity(CHECKPOINT_BUFFER_BYTES, limited);
    let mut line = Vec::with_capacity(MAX_CHECKPOINT_LINE_BYTES);
    let mut consumed = 0u64;
    let marker = read_checkpoint_line(&mut reader, &mut line, &mut consumed)?;
    validate_marker(marker, &line)?;
    if marker == LineStatus::Partial {
        return Ok(());
    }
    let mut records = 0usize;
    loop {
        let status = read_checkpoint_line(&mut reader, &mut line, &mut consumed)?;
        if status == LineStatus::End {
            return Ok(());
        }
        let text = checkpoint_line_text(&line)?;
        if !text.trim().is_empty() {
            records = records
                .checked_add(1)
                .filter(|count| *count <= MAX_CHECKPOINT_RECORDS)
                .ok_or_else(|| checkpoint_failure("checkpoint record limit exceeded"))?;
            let (peer_id, sequence, hash) = parse_checkpoint_line(text)?;
            visit(peer_id, sequence, hash)?;
        }
        if status == LineStatus::Partial {
            return Ok(());
        }
    }
}

fn read_checkpoint_line<R: BufRead>(
    reader: &mut R,
    line: &mut Vec<u8>,
    consumed: &mut u64,
) -> SyncResult<LineStatus> {
    line.clear();
    loop {
        let available = reader.fill_buf().map_err(checkpoint_io)?;
        if available.is_empty() {
            return Ok(if line.is_empty() {
                LineStatus::End
            } else {
                LineStatus::Partial
            });
        }
        let newline = available.iter().position(|byte| *byte == b'\n');
        let used = newline.map_or(available.len(), |index| index + 1);
        let body_bytes = newline.unwrap_or(available.len());
        if line.len().saturating_add(body_bytes) > MAX_CHECKPOINT_LINE_BYTES {
            return Err(checkpoint_failure("checkpoint line exceeds size limit"));
        }
        line.extend_from_slice(&available[..body_bytes]);
        reader.consume(used);
        *consumed = consumed.saturating_add(used as u64);
        if *consumed > MAX_CHECKPOINT_FILE_BYTES {
            return Err(checkpoint_failure(
                "persistent file exceeds configured limit",
            ));
        }
        if newline.is_some() {
            return Ok(LineStatus::Complete);
        }
    }
}

fn validate_marker(status: LineStatus, line: &[u8]) -> SyncResult<()> {
    let marker =
        std::str::from_utf8(line).map_err(|_| checkpoint_failure("invalid checkpoint UTF-8"))?;
    if status == LineStatus::End || marker != SYNC_CHECKPOINT_FORMAT_V1 {
        return Err(checkpoint_failure(UPDATE_REQUIRED_MESSAGE));
    }
    Ok(())
}

fn checkpoint_line_text(line: &[u8]) -> SyncResult<&str> {
    let line = line.strip_suffix(b"\r").unwrap_or(line);
    std::str::from_utf8(line).map_err(|_| checkpoint_failure("invalid checkpoint UTF-8"))
}

fn parse_checkpoint_line(line: &str) -> SyncResult<(&str, u64, &str)> {
    let (peer_id, rest) = line
        .split_once('=')
        .ok_or_else(|| checkpoint_failure("invalid checkpoint line"))?;
    validate_peer_id(peer_id)?;
    let (sequence_text, hash) = rest.split_once(',').unwrap_or((rest, ""));
    let sequence = sequence_text
        .parse::<u64>()
        .map_err(|_| checkpoint_failure("invalid checkpoint sequence"))?;
    validate_checkpoint_hash(hash)?;
    Ok((peer_id, sequence, hash))
}

fn encoded_checkpoint_bytes(map: &CheckpointMap) -> SyncResult<u64> {
    let mut bytes = (SYNC_CHECKPOINT_FORMAT_V1.len() + 1) as u64;
    for (peer_id, (sequence, hash)) in map {
        let record_bytes = peer_id
            .len()
            .checked_add(decimal_digits(*sequence))
            .and_then(|size| size.checked_add(hash.len() + 3))
            .ok_or_else(|| checkpoint_failure("checkpoint size overflow"))?;
        bytes = bytes
            .checked_add(record_bytes as u64)
            .ok_or_else(|| checkpoint_failure("checkpoint size overflow"))?;
    }
    Ok(bytes)
}

fn write_checkpoint_map(file: &mut File, map: &CheckpointMap) -> SyncResult<()> {
    let mut writer = BufWriter::with_capacity(CHECKPOINT_BUFFER_BYTES, file);
    writeln!(writer, "{SYNC_CHECKPOINT_FORMAT_V1}").map_err(checkpoint_io)?;
    for (peer_id, (sequence, hash)) in map {
        writeln!(writer, "{peer_id}={sequence},{hash}").map_err(checkpoint_io)?;
    }
    writer.flush().map_err(checkpoint_io)
}

fn checkpoint_io(error: std::io::Error) -> SyncError {
    SyncError::ReplicationFailed(error.to_string())
}

fn checkpoint_failure(message: &str) -> SyncError {
    SyncError::ReplicationFailed(message.to_string())
}

fn decimal_digits(mut value: u64) -> usize {
    let mut digits = 1;
    while value >= 10 {
        value /= 10;
        digits += 1;
    }
    digits
}
