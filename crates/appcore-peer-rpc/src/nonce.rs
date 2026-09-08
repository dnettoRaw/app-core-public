// =============================================================================
//        #######
//     ###       ###     F: nonce.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Replay-nonce persistence contracts and reference stores.

use super::PeerRpcError;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::fmt::Debug;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const NONCE_STORE_FORMAT: &str = "appcore-peer-nonce-v1";
const MAX_NONCE_STATE_BYTES: u64 = 16 * 1024 * 1024;
pub(crate) const MAX_NONCE_BYTES: usize = 128;
const SERIALIZATION_BUFFER_BYTES: usize = 64 * 1024;

/// Atomic replay protection used by [`crate::PeerRpcValidator`].
pub trait PeerNonceStore: Debug + Send + Sync {
    /// Rejects a live duplicate or records the nonce until `expires_at_ms`.
    fn check_and_record(
        &self,
        nonce: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), PeerRpcError>;
}

#[derive(Debug, Default)]
struct MemoryState {
    seen: BTreeMap<String, u64>,
    order: VecDeque<String>,
}

/// Bounded process-local nonce store used by embedded and test deployments.
#[derive(Debug, Default)]
pub struct InMemoryPeerNonceStore {
    state: Mutex<MemoryState>,
}

impl PeerNonceStore for InMemoryPeerNonceStore {
    fn check_and_record(
        &self,
        nonce: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), PeerRpcError> {
        validate_nonce(nonce)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| nonce_store_error("nonce_store_poisoned"))?;
        retain_memory(&mut state, now_ms);
        if state.seen.contains_key(nonce) {
            return Err(PeerRpcError::NonceReplay);
        }
        if state.seen.len() >= super::MAX_NONCE_CACHE_ENTRIES {
            return Err(PeerRpcError::NonceCacheFull);
        }
        state.seen.insert(nonce.to_string(), expires_at_ms);
        state.order.push_back(nonce.to_string());
        Ok(())
    }
}

/// Durable process-safe nonce store for Runtime instances sharing one volume.
#[derive(Debug, Clone)]
pub struct FilePeerNonceStore {
    path: PathBuf,
    lock_path: PathBuf,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedNonceState {
    format: String,
    entries: BTreeMap<String, u64>,
}

impl Default for PersistedNonceState {
    fn default() -> Self {
        Self {
            format: NONCE_STORE_FORMAT.to_string(),
            entries: BTreeMap::new(),
        }
    }
}

impl FilePeerNonceStore {
    /// Opens a durable nonce store and validates its owner-only layout.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, PeerRpcError> {
        let path = path.into();
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .ok_or_else(|| nonce_store_error("nonce_store_path_invalid"))?;
        create_private_directory(parent)?;
        reject_symlink(&path)?;
        let lock_path = path.with_extension("lock");
        initialize_lock(&lock_path)?;
        let store = Self { path, lock_path };
        if store.path.exists() {
            let lock = store.lock_shared()?;
            let _ = store.load()?;
            FileExt::unlock(&lock).map_err(|_| nonce_store_error("nonce_store_unlock_failed"))?;
        }
        Ok(store)
    }

    fn lock_exclusive(&self) -> Result<File, PeerRpcError> {
        validate_private_file(&self.lock_path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.lock_path)
            .map_err(|_| nonce_store_error("nonce_store_lock_unavailable"))?;
        file.lock_exclusive()
            .map_err(|_| nonce_store_error("nonce_store_lock_failed"))?;
        Ok(file)
    }

    fn lock_shared(&self) -> Result<File, PeerRpcError> {
        validate_private_file(&self.lock_path)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(&self.lock_path)
            .map_err(|_| nonce_store_error("nonce_store_lock_unavailable"))?;
        FileExt::lock_shared(&file).map_err(|_| nonce_store_error("nonce_store_lock_failed"))?;
        Ok(file)
    }

    fn load(&self) -> Result<PersistedNonceState, PeerRpcError> {
        if !self.path.exists() {
            return Ok(PersistedNonceState::default());
        }
        validate_private_file(&self.path)?;
        let file = File::open(&self.path)
            .map_err(|_| nonce_store_error("nonce_store_state_unavailable"))?;
        let length = file
            .metadata()
            .map_err(|_| nonce_store_error("nonce_store_metadata_failed"))?
            .len();
        if length == 0 || length > MAX_NONCE_STATE_BYTES {
            return Err(nonce_store_error("nonce_store_state_size_invalid"));
        }
        let mut reader = BufReader::new(file).take(MAX_NONCE_STATE_BYTES.saturating_add(1));
        let result = {
            let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
            let state = PersistedNonceState::deserialize(&mut deserializer);
            state.and_then(|state| {
                deserializer.end()?;
                Ok(state)
            })
        };
        let consumed = MAX_NONCE_STATE_BYTES
            .saturating_add(1)
            .saturating_sub(reader.limit());
        if consumed > MAX_NONCE_STATE_BYTES {
            return Err(nonce_store_error("nonce_store_state_size_invalid"));
        }
        let state = result.map_err(|_| nonce_store_error("nonce_store_state_corrupt"))?;
        if state.format != NONCE_STORE_FORMAT
            || state.entries.len() > super::MAX_NONCE_CACHE_ENTRIES
            || state.entries.keys().any(|nonce| !nonce_is_valid(nonce))
        {
            return Err(nonce_store_error("nonce_store_format_invalid"));
        }
        Ok(state)
    }

    fn persist(&self, state: &PersistedNonceState) -> Result<(), PeerRpcError> {
        atomic_write(&self.path, state)
    }
}

impl PeerNonceStore for FilePeerNonceStore {
    fn check_and_record(
        &self,
        nonce: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), PeerRpcError> {
        validate_nonce(nonce)?;
        let lock = self.lock_exclusive()?;
        let mut state = self.load()?;
        state
            .entries
            .retain(|_, stored_expiry| *stored_expiry > now_ms);
        if state.entries.contains_key(nonce) {
            return Err(PeerRpcError::NonceReplay);
        }
        if state.entries.len() >= super::MAX_NONCE_CACHE_ENTRIES {
            return Err(PeerRpcError::NonceCacheFull);
        }
        state.entries.insert(nonce.to_string(), expires_at_ms);
        self.persist(&state)?;
        FileExt::unlock(&lock).map_err(|_| nonce_store_error("nonce_store_unlock_failed"))
    }
}

fn retain_memory(state: &mut MemoryState, now_ms: u64) {
    while let Some(front) = state.order.front() {
        let expired = state
            .seen
            .get(front)
            .map(|expires_at_ms| *expires_at_ms <= now_ms)
            .unwrap_or(true);
        if !expired {
            break;
        }
        if let Some(key) = state.order.pop_front() {
            state.seen.remove(&key);
        }
    }
}

fn initialize_lock(path: &Path) -> Result<(), PeerRpcError> {
    reject_symlink(path)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)
        .map_err(|_| nonce_store_error("nonce_store_lock_create_failed"))?;
    set_private_file_permissions(&file)
}

fn atomic_write(path: &Path, state: &PersistedNonceState) -> Result<(), PeerRpcError> {
    let parent = path
        .parent()
        .ok_or_else(|| nonce_store_error("nonce_store_path_invalid"))?;
    validate_private_directory(parent)?;
    reject_symlink(path)?;
    let temporary = parent.join(format!(".nonce-{}-{}.tmp", std::process::id(), now_nanos()));
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&temporary)
        .map_err(|_| nonce_store_error("nonce_store_temp_create_failed"))?;
    let write_result = (|| {
        set_private_file_permissions(&file)?;
        write_bounded_state(&mut file, state)?;
        file.sync_all()
            .map_err(|_| nonce_store_error("nonce_store_write_failed"))
    })();
    drop(file);
    let result = write_result
        .and_then(|()| replace_file(&temporary, path))
        .and_then(|()| {
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| nonce_store_error("nonce_store_directory_sync_failed"))
        });
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(not(windows))]
fn replace_file(temporary: &Path, target: &Path) -> Result<(), PeerRpcError> {
    fs::rename(temporary, target).map_err(|_| nonce_store_error("nonce_store_replace_failed"))
}

#[cfg(windows)]
fn replace_file(temporary: &Path, target: &Path) -> Result<(), PeerRpcError> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MoveFileExW, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    };
    let source = temporary
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let destination = target
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    if unsafe {
        MoveFileExW(
            source.as_ptr(),
            destination.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    } == 0
    {
        return Err(nonce_store_error("nonce_store_replace_failed"));
    }
    Ok(())
}

fn write_bounded_state(file: &mut File, state: &PersistedNonceState) -> Result<(), PeerRpcError> {
    let mut writer = BoundedWriter::new(file, MAX_NONCE_STATE_BYTES);
    let result = {
        let mut buffered = BufWriter::with_capacity(SERIALIZATION_BUFFER_BYTES, &mut writer);
        let result = serde_json::to_writer(&mut buffered, state);
        result.and_then(|()| buffered.flush().map_err(serde_json::Error::io))
    };
    if writer.exceeded {
        return Err(nonce_store_error("nonce_store_state_size_invalid"));
    }
    result.map_err(|error| {
        if error.is_io() {
            nonce_store_error("nonce_store_write_failed")
        } else {
            nonce_store_error("nonce_store_encode_failed")
        }
    })
}

struct BoundedWriter<W> {
    inner: W,
    remaining: u64,
    exceeded: bool,
}

impl<W> BoundedWriter<W> {
    const fn new(inner: W, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
            exceeded: false,
        }
    }
}

impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            self.exceeded = true;
            return Err(std::io::Error::other(
                "nonce state exceeds configured limit",
            ));
        }
        let written = self.inner.write(bytes)?;
        self.remaining = self.remaining.saturating_sub(written as u64);
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

pub(crate) fn validate_nonce(nonce: &str) -> Result<(), PeerRpcError> {
    if nonce_is_valid(nonce) {
        Ok(())
    } else {
        Err(nonce_store_error("nonce_store_nonce_invalid"))
    }
}

fn nonce_is_valid(nonce: &str) -> bool {
    !nonce.is_empty()
        && nonce.len() <= MAX_NONCE_BYTES
        && appcore_core::validate_identifier("PeerNonce", nonce).is_ok()
}

fn create_private_directory(path: &Path) -> Result<(), PeerRpcError> {
    reject_symlink(path)?;
    fs::create_dir_all(path)
        .map_err(|_| nonce_store_error("nonce_store_directory_create_failed"))?;
    set_private_directory_permissions(path)?;
    validate_private_directory(path)
}

fn reject_symlink(path: &Path) -> Result<(), PeerRpcError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(nonce_store_error("nonce_store_symlink_rejected"))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(nonce_store_error("nonce_store_metadata_failed")),
    }
}

#[cfg(unix)]
fn set_private_directory_permissions(path: &Path) -> Result<(), PeerRpcError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| nonce_store_error("nonce_store_permissions_failed"))
}

#[cfg(not(unix))]
fn set_private_directory_permissions(_path: &Path) -> Result<(), PeerRpcError> {
    Ok(())
}

#[cfg(unix)]
fn set_private_file_permissions(file: &File) -> Result<(), PeerRpcError> {
    use std::os::unix::fs::PermissionsExt;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|_| nonce_store_error("nonce_store_permissions_failed"))
}

#[cfg(not(unix))]
fn set_private_file_permissions(_file: &File) -> Result<(), PeerRpcError> {
    Ok(())
}

#[cfg(unix)]
fn validate_private_directory(path: &Path) -> Result<(), PeerRpcError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| nonce_store_error("nonce_store_directory_unavailable"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_dir()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(nonce_store_error("nonce_store_directory_insecure"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_directory(path: &Path) -> Result<(), PeerRpcError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| nonce_store_error("nonce_store_directory_unavailable"))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(nonce_store_error("nonce_store_directory_insecure"));
    }
    Ok(())
}

#[cfg(unix)]
fn validate_private_file(path: &Path) -> Result<(), PeerRpcError> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| nonce_store_error("nonce_store_file_unavailable"))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(nonce_store_error("nonce_store_file_insecure"));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_file(path: &Path) -> Result<(), PeerRpcError> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| nonce_store_error("nonce_store_file_unavailable"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(nonce_store_error("nonce_store_file_insecure"));
    }
    Ok(())
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

fn nonce_store_error(reason: &'static str) -> PeerRpcError {
    PeerRpcError::InvalidEnvelope(reason.to_string())
}

#[cfg(test)]
#[path = "nonce_tests.rs"]
mod tests;
