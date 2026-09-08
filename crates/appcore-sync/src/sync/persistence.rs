// =============================================================================
//        #######
//     ###       ###     F: persistence.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 13:24:05 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Defines bounded persistence contracts and behavior for this crate.

use crate::sync::error::{SyncError, SyncResult};
use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

pub(super) struct PersistenceLock {
    file: File,
}

impl Drop for PersistenceLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

pub(super) fn atomic_write(path: &Path, bytes: &[u8]) -> SyncResult<()> {
    atomic_write_with(path, |file| {
        file.write_all(bytes)
            .map_err(|error| SyncError::ReplicationFailed(error.to_string()))
    })
}

pub(super) fn atomic_write_with(
    path: &Path,
    write: impl FnOnce(&mut File) -> SyncResult<()>,
) -> SyncResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
    reject_symlink(path)?;
    let temporary = temporary_path(path, parent);
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
        write(&mut file)?;
        file.sync_all()
            .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
        fs::rename(&temporary, path)
            .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
        sync_parent(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

pub(super) fn truncate_synced(path: &Path, length: u64) -> SyncResult<()> {
    reject_symlink(path)?;
    let file = OpenOptions::new()
        .write(true)
        .open(path)
        .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
    file.set_len(length)
        .and_then(|_| file.sync_all())
        .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
    sync_parent(path.parent().unwrap_or_else(|| Path::new(".")))
}

pub(super) fn acquire_persistence_lock(path: &Path) -> SyncResult<PersistenceLock> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
    let lock_path = lock_path(path, parent);
    reject_symlink(&lock_path)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(lock_path)
        .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
    file.lock_exclusive()
        .map_err(|error| SyncError::ReplicationFailed(error.to_string()))?;
    Ok(PersistenceLock { file })
}

pub(super) fn reject_symlink(path: &Path) -> SyncResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => Err(SyncError::ReplicationFailed(
            "persistent path must not be a symlink".to_string(),
        )),
        Ok(metadata) if !metadata.is_file() => Err(SyncError::ReplicationFailed(
            "persistent path is not a regular file".to_string(),
        )),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(SyncError::ReplicationFailed(error.to_string())),
    }
}

fn temporary_path(path: &Path, parent: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("appcore-sync");
    parent.join(format!(
        ".{name}.{}-{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ))
}

fn lock_path(path: &Path, parent: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("appcore-sync");
    parent.join(format!(".{name}.lock"))
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> SyncResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| SyncError::ReplicationFailed(error.to_string()))
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> SyncResult<()> {
    Ok(())
}
