// =============================================================================
//        #######
//     ###       ###     F: storage_backup.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Versioned whole-provider backup, verification, restore, and crash recovery.

use super::storage_file_fs::{
    create_new_file, ensure_real_directory, fsync_parent, open_lock_file, open_regular_file,
    path_exists_no_follow, resolve_under_root, sync_directory, tmp_path_for, write_atomic_file,
};
use super::storage_tree::{bounded_tree_entries, StorageTreeEntryKind};
use super::{BackupDescriptor, FileStorageProvider, StorageError, StorageResult};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable format identifier for complete local storage snapshots.
pub const STORAGE_BACKUP_FORMAT_V1: &str = "appcore-storage-backup-v1";
pub(super) const BACKUP_MANIFEST: &str = "manifest.json";
const BACKUP_DATA: &str = "data";
const MAX_BACKUP_FILES: usize = 100_000;
const MAX_BACKUP_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

/// One file recorded by a V1 storage backup manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageBackupManifestFileV1 {
    /// Slash-separated path relative to the provider data root.
    pub path: String,
    /// File size in bytes.
    pub size: u64,
    /// Lowercase SHA-256 digest of the complete file.
    pub sha256: String,
}

/// Durable manifest for one complete V1 storage snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StorageBackupManifestV1 {
    /// Stable persisted format identifier.
    pub format: String,
    /// Backup name selected by the operator.
    pub name: String,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: u64,
    /// Complete sorted file inventory.
    pub files: Vec<StorageBackupManifestFileV1>,
}

struct StorageOperationLock {
    file: File,
}

impl Drop for StorageOperationLock {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

impl FileStorageProvider {
    /// Creates an atomic, checksummed snapshot of the complete data root.
    pub fn create_snapshot_backup(&self, name: &str) -> StorageResult<BackupDescriptor> {
        self.create_dirs()?;
        self.with_storage_lock(|| self.create_snapshot_locked(name))
    }

    /// Verifies a complete snapshot without changing current data.
    pub fn verify_snapshot_backup(&self, name: &str) -> StorageResult<BackupDescriptor> {
        self.with_storage_lock(|| {
            let (_, manifest) = self.load_verified_backup(name)?;
            Ok(descriptor(&manifest))
        })
    }

    /// Restores a verified snapshot through a recoverable directory swap.
    pub fn restore_snapshot_backup(&self, name: &str) -> StorageResult<BackupDescriptor> {
        self.create_dirs()?;
        self.with_storage_lock(|| self.restore_snapshot_locked(name))
    }

    /// Resolves interrupted restore phases without discarding the last good root.
    pub fn recover_snapshot_restore(&self) -> StorageResult<()> {
        self.with_storage_lock(|| self.recover_restore_locked())
    }

    pub(super) fn with_storage_lock<T>(
        &self,
        operation: impl FnOnce() -> StorageResult<T>,
    ) -> StorageResult<T> {
        let _lock = self.acquire_operation_lock()?;
        operation()
    }

    fn create_snapshot_locked(&self, name: &str) -> StorageResult<BackupDescriptor> {
        validate_backup_name(name)?;
        reject_overlapping_roots(&self.storage_path, &self.backup_path)?;
        let final_path = resolve_under_root(&self.backup_path, name)?;
        if path_exists_no_follow(&final_path)? {
            return Err(StorageError::BackupFailed(name.to_string()));
        }
        let staging = snapshot_staging_path(&final_path);
        fs::create_dir(&staging).map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        ensure_real_directory(&staging)?;
        let result = self.populate_snapshot(name, &staging);
        if result.is_err() {
            let _ = fs::remove_dir_all(&staging);
        }
        let manifest = result?;
        if path_exists_no_follow(&final_path)? {
            return Err(StorageError::BackupFailed(name.to_string()));
        }
        ensure_real_directory(&staging)?;
        fs::rename(&staging, &final_path)
            .map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        fsync_parent(&final_path).map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        Ok(descriptor(&manifest))
    }

    fn populate_snapshot(
        &self,
        name: &str,
        staging: &Path,
    ) -> StorageResult<StorageBackupManifestV1> {
        let data_root = staging.join(BACKUP_DATA);
        fs::create_dir(&data_root).map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        let mut files = Vec::new();
        collect_regular_files(&self.storage_path, &mut files)?;
        files.sort();
        if files.len() > MAX_BACKUP_FILES {
            return Err(StorageError::BackupFailed(name.to_string()));
        }
        let mut entries = Vec::with_capacity(files.len());
        for relative in files {
            entries.push(copy_snapshot_file(
                &self.storage_path,
                &data_root,
                &relative,
            )?);
        }
        let manifest = StorageBackupManifestV1 {
            format: STORAGE_BACKUP_FORMAT_V1.to_string(),
            name: name.to_string(),
            created_at_ms: now_ms(),
            files: entries,
        };
        write_manifest(staging, &manifest)?;
        sync_directory_tree(&data_root)?;
        sync_directory(staging).map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        Ok(manifest)
    }

    fn restore_snapshot_locked(&self, name: &str) -> StorageResult<BackupDescriptor> {
        self.recover_restore_locked()?;
        let (backup_root, manifest) = self.load_verified_backup(name)?;
        let pending = self.restore_pending_path()?;
        let previous = self.restore_previous_path()?;
        copy_verified_tree(&backup_root.join(BACKUP_DATA), &pending, &manifest)?;
        fsync_parent(&pending).map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        fs::rename(&self.storage_path, &previous)
            .map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        fsync_parent(&previous).map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        if fs::rename(&pending, &self.storage_path).is_err() {
            let _ = fs::rename(&previous, &self.storage_path);
            return Err(StorageError::BackupFailed(name.to_string()));
        }
        fsync_parent(&self.storage_path)
            .map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        fs::remove_dir_all(&previous).map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        fsync_parent(&self.storage_path)
            .map_err(|_| StorageError::BackupFailed(name.to_string()))?;
        Ok(descriptor(&manifest))
    }

    fn load_verified_backup(
        &self,
        name: &str,
    ) -> StorageResult<(PathBuf, StorageBackupManifestV1)> {
        validate_backup_name(name)?;
        let root = resolve_under_root(&self.backup_path, name)?;
        reject_symlink_tree(&root)?;
        let manifest = read_manifest(&root, name)?;
        verify_manifest(name, &root.join(BACKUP_DATA), &manifest)?;
        Ok((root, manifest))
    }

    fn recover_restore_locked(&self) -> StorageResult<()> {
        let pending = self.restore_pending_path()?;
        let previous = self.restore_previous_path()?;
        if !real_directory_exists(&self.storage_path)? {
            if real_directory_exists(&pending)? {
                fs::rename(&pending, &self.storage_path).map_err(|_| StorageError::NotAvailable)?;
            } else if real_directory_exists(&previous)? {
                fs::rename(&previous, &self.storage_path)
                    .map_err(|_| StorageError::NotAvailable)?;
            }
        }
        if real_directory_exists(&self.storage_path)? && real_directory_exists(&pending)? {
            fs::remove_dir_all(&pending).map_err(|_| StorageError::NotAvailable)?;
        }
        if real_directory_exists(&self.storage_path)? && real_directory_exists(&previous)? {
            fs::remove_dir_all(&previous).map_err(|_| StorageError::NotAvailable)?;
        }
        if real_directory_exists(&self.storage_path)? {
            fsync_parent(&self.storage_path).map_err(|_| StorageError::NotAvailable)?;
        }
        Ok(())
    }

    fn acquire_operation_lock(&self) -> StorageResult<StorageOperationLock> {
        let path = sibling_path(&self.storage_path, "lock")?;
        let file = open_lock_file(&path).map_err(|_| StorageError::NotAvailable)?;
        file.lock_exclusive()
            .map_err(|_| StorageError::NotAvailable)?;
        Ok(StorageOperationLock { file })
    }

    fn restore_pending_path(&self) -> StorageResult<PathBuf> {
        sibling_path(&self.storage_path, "restore.pending")
    }

    fn restore_previous_path(&self) -> StorageResult<PathBuf> {
        sibling_path(&self.storage_path, "restore.previous")
    }
}

fn collect_regular_files(root: &Path, output: &mut Vec<PathBuf>) -> StorageResult<()> {
    for entry in bounded_tree_entries(root)? {
        if entry.kind == StorageTreeEntryKind::Link {
            return Err(StorageError::InvalidPath(entry.path.display().to_string()));
        }
        if entry.kind == StorageTreeEntryKind::File {
            output.push(
                entry
                    .path
                    .strip_prefix(root)
                    .map_err(|_| StorageError::InvalidPath(entry.path.display().to_string()))?
                    .to_path_buf(),
            );
        }
    }
    Ok(())
}

fn copy_snapshot_file(
    source_root: &Path,
    destination_root: &Path,
    relative: &Path,
) -> StorageResult<StorageBackupManifestFileV1> {
    let portable = portable_path(relative)?;
    let source = resolve_under_root(source_root, &portable)?;
    let destination = resolve_under_root(destination_root, &portable)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|_| StorageError::BackupFailed(path_text(relative)))?;
        ensure_real_directory(parent)?;
    }
    copy_and_sync(&source, &destination)?;
    let (size, sha256) = hash_file(&destination)?;
    Ok(StorageBackupManifestFileV1 {
        path: portable,
        size,
        sha256,
    })
}

fn copy_verified_tree(
    source_root: &Path,
    destination_root: &Path,
    manifest: &StorageBackupManifestV1,
) -> StorageResult<()> {
    if path_exists_no_follow(destination_root)? {
        ensure_real_directory(destination_root)?;
        fs::remove_dir_all(destination_root).map_err(|_| StorageError::NotAvailable)?;
    }
    fs::create_dir(destination_root).map_err(|_| StorageError::NotAvailable)?;
    for entry in &manifest.files {
        validated_manifest_path(&entry.path)?;
        let source = resolve_under_root(source_root, &entry.path)?;
        let destination = resolve_under_root(destination_root, &entry.path)?;
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|_| StorageError::NotAvailable)?;
            ensure_real_directory(parent)?;
        }
        copy_and_sync(&source, &destination)?;
    }
    sync_directory_tree(destination_root)
}

fn copy_and_sync(source: &Path, destination: &Path) -> StorageResult<()> {
    let mut source = open_regular_file(source).map_err(|_| StorageError::NotAvailable)?;
    let mut destination = create_new_file(destination).map_err(|_| StorageError::NotAvailable)?;
    io::copy(&mut source, &mut destination).map_err(|_| StorageError::NotAvailable)?;
    destination
        .sync_all()
        .map_err(|_| StorageError::NotAvailable)?;
    Ok(())
}

fn write_manifest(root: &Path, manifest: &StorageBackupManifestV1) -> StorageResult<()> {
    let bytes = serde_json::to_vec_pretty(manifest)
        .map_err(|_| StorageError::BackupFailed(manifest.name.clone()))?;
    let path = root.join(BACKUP_MANIFEST);
    write_atomic_file(&tmp_path_for(&path), &path, &bytes)
        .map_err(|_| StorageError::BackupFailed(manifest.name.clone()))
}

pub(super) fn read_manifest(root: &Path, name: &str) -> StorageResult<StorageBackupManifestV1> {
    let path = root.join(BACKUP_MANIFEST);
    let mut file =
        open_regular_file(&path).map_err(|_| StorageError::BackupFailed(name.to_string()))?;
    let metadata = file
        .metadata()
        .map_err(|_| StorageError::BackupFailed(name.to_string()))?;
    if metadata.len() > MAX_BACKUP_MANIFEST_BYTES {
        return Err(StorageError::BackupFailed(name.to_string()));
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.read_to_end(&mut bytes)
        .map_err(|_| StorageError::BackupFailed(name.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|_| StorageError::BackupFailed(name.to_string()))
}

fn verify_manifest(
    name: &str,
    data_root: &Path,
    manifest: &StorageBackupManifestV1,
) -> StorageResult<()> {
    if manifest.format != STORAGE_BACKUP_FORMAT_V1
        || manifest.name != name
        || manifest.files.len() > MAX_BACKUP_FILES
    {
        return Err(StorageError::BackupFailed(name.to_string()));
    }
    let mut previous = None;
    for entry in &manifest.files {
        let relative = validated_manifest_path(&entry.path)?;
        if previous.as_ref().is_some_and(|path| path >= &entry.path) {
            return Err(StorageError::BackupFailed(name.to_string()));
        }
        let (size, sha256) = hash_file(&data_root.join(&relative))?;
        if size != entry.size || sha256 != entry.sha256 {
            return Err(StorageError::BackupFailed(name.to_string()));
        }
        previous = Some(entry.path.clone());
    }
    let mut actual = Vec::new();
    collect_regular_files(data_root, &mut actual)?;
    if actual.len() != manifest.files.len() {
        return Err(StorageError::BackupFailed(name.to_string()));
    }
    Ok(())
}

fn hash_file(path: &Path) -> StorageResult<(u64, String)> {
    let mut file = open_regular_file(path).map_err(|_| StorageError::NotAvailable)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| StorageError::NotAvailable)?;
        if read == 0 {
            break;
        }
        size = size.saturating_add(read as u64);
        hasher.update(&buffer[..read]);
    }
    Ok((size, format!("{:x}", hasher.finalize())))
}

fn reject_symlink_tree(root: &Path) -> StorageResult<()> {
    let mut files = Vec::new();
    collect_regular_files(root, &mut files)
}

fn reject_overlapping_roots(storage: &Path, backup: &Path) -> StorageResult<()> {
    let storage = fs::canonicalize(storage).map_err(|_| StorageError::NotAvailable)?;
    let backup = fs::canonicalize(backup).map_err(|_| StorageError::NotAvailable)?;
    if storage.starts_with(&backup) || backup.starts_with(&storage) {
        return Err(StorageError::InvalidPath(backup.display().to_string()));
    }
    Ok(())
}

fn validate_backup_name(name: &str) -> StorageResult<()> {
    let path = Path::new(name);
    if name.is_empty() || path.components().count() != 1 || name.starts_with('.') {
        return Err(StorageError::InvalidPath(name.to_string()));
    }
    match path.components().next() {
        Some(std::path::Component::Normal(_)) => Ok(()),
        _ => Err(StorageError::InvalidPath(name.to_string())),
    }
}

fn validated_manifest_path(path: &str) -> StorageResult<PathBuf> {
    let relative = Path::new(path);
    if path.is_empty()
        || relative.is_absolute()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(StorageError::InvalidPath(path.to_string()));
    }
    Ok(relative.to_path_buf())
}

fn portable_path(path: &Path) -> StorageResult<String> {
    let parts: Option<Vec<_>> = path
        .components()
        .map(|component| match component {
            std::path::Component::Normal(part) => part.to_str(),
            _ => None,
        })
        .collect();
    parts
        .map(|parts| parts.join("/"))
        .filter(|path| !path.is_empty())
        .ok_or_else(|| StorageError::InvalidPath(path.display().to_string()))
}

fn sync_directory_tree(root: &Path) -> StorageResult<()> {
    let mut directories = vec![root.to_path_buf()];
    for entry in bounded_tree_entries(root)? {
        match entry.kind {
            StorageTreeEntryKind::Directory => directories.push(entry.path),
            StorageTreeEntryKind::Link => {
                return Err(StorageError::InvalidPath(entry.path.display().to_string()));
            }
            StorageTreeEntryKind::File | StorageTreeEntryKind::Other => {}
        }
    }
    for directory in directories.into_iter().rev() {
        sync_directory(&directory).map_err(|_| StorageError::NotAvailable)?;
    }
    Ok(())
}

fn sibling_path(path: &Path, suffix: &str) -> StorageResult<PathBuf> {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| StorageError::InvalidPath(path.display().to_string()))?;
    Ok(path.with_file_name(format!("{name}.{suffix}")))
}

fn snapshot_staging_path(path: &Path) -> PathBuf {
    let candidate = tmp_path_for(path);
    candidate.with_extension("snapshot.tmp")
}

pub(super) fn descriptor(manifest: &StorageBackupManifestV1) -> BackupDescriptor {
    BackupDescriptor {
        name: manifest.name.clone(),
        created_at_ms: manifest.created_at_ms,
    }
}

fn real_directory_exists(path: &Path) -> StorageResult<bool> {
    if !path_exists_no_follow(path)? {
        return Ok(false);
    }
    ensure_real_directory(path)?;
    Ok(true)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn path_text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
