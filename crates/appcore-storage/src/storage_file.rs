// =============================================================================
//        #######
//     ###       ###     F: storage_file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 12:02:44 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::storage_backup_list::list_backup_descriptors;
use super::storage_file_fs::{
    create_real_directory_all, open_regular_file, path_exists_no_follow, path_is_real_directory,
    resolve_under_root, tmp_path_for, write_atomic_file,
};
use super::storage_tree::{bounded_tree_entries, StorageTreeEntryKind};
use super::*;
use crate::storage::RemoteAuthStorageClient;
use appcore_security::{TokenClaims, TokenProvider};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Single-process local file provider with atomic replacement writes.
#[derive(Debug, Clone)]
pub struct FileStorageProvider {
    pub(super) storage_path: PathBuf,
    pub(super) backup_path: PathBuf,
    opened: bool,
}

impl FileStorageProvider {
    /// Creates a provider with separate data and backup roots.
    pub fn new(storage_path: impl Into<PathBuf>, backup_path: impl Into<PathBuf>) -> Self {
        Self {
            storage_path: storage_path.into(),
            backup_path: backup_path.into(),
            opened: false,
        }
    }

    /// Creates the configured data and backup directories.
    pub fn create_dirs(&self) -> StorageResult<()> {
        if let Some(parent) = self.storage_path.parent() {
            fs::create_dir_all(parent).map_err(|_| StorageError::NotAvailable)?;
        }
        create_real_directory_all(&self.backup_path)?;
        if path_exists_no_follow(&self.storage_path)? {
            super::storage_file_fs::ensure_real_directory(&self.storage_path)?;
        }
        self.recover_snapshot_restore()?;
        create_real_directory_all(&self.storage_path)?;
        Ok(())
    }

    /// Atomically writes bytes below the data root.
    pub fn write_bytes(&self, path: &str, bytes: &[u8]) -> StorageResult<()> {
        self.write_bytes_atomic(path, bytes)
    }

    /// Writes through an exclusive temporary file, fsync and atomic rename.
    pub fn write_bytes_atomic(&self, path: &str, bytes: &[u8]) -> StorageResult<()> {
        self.resolve_storage(path)?;
        self.with_storage_lock(|| {
            let full = self.resolve_storage(path)?;
            if let Some(parent) = full.parent() {
                fs::create_dir_all(parent).map_err(|_| StorageError::NotAvailable)?;
            }
            let full = self.resolve_storage(path)?;
            let tmp = tmp_path_for(&full);
            write_atomic_file(&tmp, &full, bytes)
                .map_err(|_| StorageError::TransactionFailed(path.to_string()))
        })?;
        Ok(())
    }

    /// Seals and atomically writes bytes using an explicit token provider.
    pub fn write_secure_bytes<P: TokenProvider>(
        &self,
        path: &str,
        bytes: &[u8],
        provider: &P,
        claims: &TokenClaims,
    ) -> StorageResult<()> {
        let sealed = provider
            .seal(bytes, claims)
            .map_err(|_| StorageError::SecurityFailed(path.to_string()))?;
        self.write_bytes_atomic(path, &sealed)
    }

    /// Writes sealed bytes and fails when authentication is not configured.
    pub fn write_auth_required_bytes<P: TokenProvider>(
        &self,
        path: &str,
        bytes: &[u8],
        provider: Option<&P>,
        claims: &TokenClaims,
    ) -> StorageResult<()> {
        let provider = require_auth_provider(path, provider)?;
        self.write_secure_bytes(path, bytes, provider, claims)
    }

    /// Seals bytes through a remote authentication service before writing.
    pub fn write_remote_auth_required_bytes(
        &self,
        path: &str,
        bytes: &[u8],
        client: Option<&RemoteAuthStorageClient>,
    ) -> StorageResult<()> {
        let client = require_remote_auth_client(path, client)?;
        let sealed = client.seal_resource(path, bytes)?;
        self.write_bytes_atomic(path, &sealed)
    }

    /// Reads bytes below the data root.
    pub fn read_bytes(&self, path: &str) -> StorageResult<Vec<u8>> {
        self.resolve_storage(path)?;
        self.with_storage_lock(|| {
            let full = self.resolve_storage(path)?;
            let mut file = open_regular_file(&full)
                .map_err(|_| StorageError::RepositoryNotFound(path.to_string()))?;
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes)
                .map_err(|_| StorageError::RepositoryNotFound(path.to_string()))?;
            Ok(bytes)
        })
    }

    pub(super) fn read_bytes_bounded(&self, path: &str, max_bytes: u64) -> StorageResult<Vec<u8>> {
        self.resolve_storage(path)?;
        self.with_storage_lock(|| {
            let full = self.resolve_storage(path)?;
            let file = open_regular_file(&full)
                .map_err(|_| StorageError::RepositoryNotFound(path.to_string()))?;
            let metadata = file
                .metadata()
                .map_err(|_| StorageError::RepositoryNotFound(path.to_string()))?;
            if metadata.len() > max_bytes {
                return Err(StorageError::TransactionFailed(path.to_string()));
            }
            let capacity = usize::try_from(metadata.len())
                .map_err(|_| StorageError::TransactionFailed(path.to_string()))?;
            let read_limit = max_bytes
                .checked_add(1)
                .ok_or_else(|| StorageError::TransactionFailed(path.to_string()))?;
            let mut bytes = Vec::with_capacity(capacity);
            file.take(read_limit)
                .read_to_end(&mut bytes)
                .map_err(|_| StorageError::RepositoryNotFound(path.to_string()))?;
            if bytes.len() as u64 > max_bytes {
                return Err(StorageError::TransactionFailed(path.to_string()));
            }
            Ok(bytes)
        })
    }

    /// Reads and opens sealed bytes using an explicit token provider.
    pub fn read_secure_bytes<P: TokenProvider>(
        &self,
        path: &str,
        provider: &P,
        claims: &TokenClaims,
    ) -> StorageResult<Vec<u8>> {
        let sealed = self.read_bytes(path)?;
        provider
            .open(&sealed, claims)
            .map_err(|_| StorageError::SecurityFailed(path.to_string()))
    }

    /// Reads sealed bytes and fails when authentication is not configured.
    pub fn read_auth_required_bytes<P: TokenProvider>(
        &self,
        path: &str,
        provider: Option<&P>,
        claims: &TokenClaims,
    ) -> StorageResult<Vec<u8>> {
        let provider = require_auth_provider(path, provider)?;
        self.read_secure_bytes(path, provider, claims)
    }

    /// Reads and opens bytes through a remote authentication service.
    pub fn read_remote_auth_required_bytes(
        &self,
        path: &str,
        client: Option<&RemoteAuthStorageClient>,
    ) -> StorageResult<Vec<u8>> {
        let client = require_remote_auth_client(path, client)?;
        let sealed = self.read_bytes(path)?;
        client.open_resource(path, &sealed)
    }

    /// Reports whether a relative data path exists.
    pub fn exists(&self, path: &str) -> StorageResult<bool> {
        self.resolve_storage(path)?;
        self.with_storage_lock(|| {
            let full = self.resolve_storage(path)?;
            path_exists_no_follow(&full)
        })
    }

    /// Atomically copies a data file below the backup root.
    pub fn backup_file(&self, source: &str, backup_name: &str) -> StorageResult<()> {
        self.backup_file_atomic(source, backup_name)
    }

    /// Creates a backup through fsync and atomic replacement.
    pub fn backup_file_atomic(&self, source: &str, backup_name: &str) -> StorageResult<()> {
        self.resolve_storage(source)?;
        self.resolve_backup(backup_name)?;
        self.with_storage_lock(|| {
            let source_full = self.resolve_storage(source)?;
            if !path_exists_no_follow(&source_full)? {
                return Err(StorageError::RepositoryNotFound(source.to_string()));
            }
            let backup_full = self.resolve_backup(backup_name)?;
            if let Some(parent) = backup_full.parent() {
                fs::create_dir_all(parent)
                    .map_err(|_| StorageError::BackupFailed(backup_name.to_string()))?;
            }
            let backup_full = self.resolve_backup(backup_name)?;
            let mut source_file = open_regular_file(&source_full)
                .map_err(|_| StorageError::BackupFailed(backup_name.to_string()))?;
            let mut bytes = Vec::new();
            source_file
                .read_to_end(&mut bytes)
                .map_err(|_| StorageError::BackupFailed(backup_name.to_string()))?;
            let tmp = tmp_path_for(&backup_full);
            write_atomic_file(&tmp, &backup_full, &bytes)
                .map_err(|_| StorageError::BackupFailed(backup_name.to_string()))
        })?;
        Ok(())
    }

    /// Removes orphaned temporary files below data and backup roots.
    pub fn cleanup_temp_files(&self) -> StorageResult<usize> {
        self.with_storage_lock(|| {
            let mut removed = cleanup_tmp_in_dir(&self.storage_path)?;
            removed += cleanup_tmp_in_dir(&self.backup_path)?;
            Ok(removed)
        })
    }

    fn resolve_storage(&self, relative: &str) -> StorageResult<PathBuf> {
        resolve_under_root(&self.storage_path, relative)
    }

    fn resolve_backup(&self, relative: &str) -> StorageResult<PathBuf> {
        resolve_under_root(&self.backup_path, relative)
    }
}

fn require_auth_provider<'a, P: TokenProvider>(
    path: &str,
    provider: Option<&'a P>,
) -> StorageResult<&'a P> {
    provider.ok_or_else(|| StorageError::AuthUnavailable(path.to_string()))
}

fn require_remote_auth_client<'a>(
    path: &str,
    client: Option<&'a RemoteAuthStorageClient>,
) -> StorageResult<&'a RemoteAuthStorageClient> {
    client.ok_or_else(|| StorageError::AuthUnavailable(path.to_string()))
}

fn cleanup_tmp_in_dir(root: &Path) -> StorageResult<usize> {
    if !path_exists_no_follow(root)? {
        return Ok(0);
    }
    let mut removed = 0usize;
    for entry in bounded_tree_entries(root)? {
        if entry.kind == StorageTreeEntryKind::File
            && entry
                .path
                .file_name()
                .map(|name| name.to_string_lossy().ends_with(".tmp"))
                .unwrap_or(false)
        {
            fs::remove_file(entry.path).map_err(|_| StorageError::NotAvailable)?;
            removed += 1;
        }
    }
    Ok(removed)
}

fn count_tmp_in_dir(root: &Path) -> StorageResult<usize> {
    if !path_exists_no_follow(root)? {
        return Ok(0);
    }
    let mut count = 0usize;
    for entry in bounded_tree_entries(root)? {
        if entry.kind == StorageTreeEntryKind::File
            && entry
                .path
                .file_name()
                .map(|name| name.to_string_lossy().ends_with(".tmp"))
                .unwrap_or(false)
        {
            count += 1;
        }
    }
    Ok(count)
}

impl StorageProvider for FileStorageProvider {
    fn status(&self) -> StorageStatus {
        if path_is_real_directory(&self.storage_path) && path_is_real_directory(&self.backup_path) {
            StorageStatus::Online
        } else {
            StorageStatus::Offline
        }
    }

    fn health(&self) -> StorageHealth {
        if path_is_real_directory(&self.storage_path) && path_is_real_directory(&self.backup_path) {
            let orphan_tmp = match (
                count_tmp_in_dir(&self.storage_path),
                count_tmp_in_dir(&self.backup_path),
            ) {
                (Ok(storage), Ok(backups)) => storage.saturating_add(backups),
                _ => {
                    return StorageHealth {
                        status: StorageStatus::Degraded,
                        message: Some("temporary file scan failed".to_string()),
                    };
                }
            };
            if orphan_tmp > 0 {
                return StorageHealth {
                    status: StorageStatus::Degraded,
                    message: Some(format!("found {orphan_tmp} orphan temp files")),
                };
            }
            return StorageHealth {
                status: StorageStatus::Online,
                message: None,
            };
        }
        StorageHealth {
            status: StorageStatus::Degraded,
            message: Some("storage or backup path is missing".to_string()),
        }
    }

    fn open(&mut self) -> StorageResult<()> {
        self.create_dirs()?;
        self.opened = true;
        Ok(())
    }

    fn close(&mut self) -> StorageResult<()> {
        self.opened = false;
        Ok(())
    }

    fn begin_transaction(&mut self) -> StorageResult<Box<dyn Transaction>> {
        if !self.opened {
            return Err(StorageError::NotAvailable);
        }
        Err(StorageError::TransactionsUnsupported)
    }

    fn list_backups(&self) -> Vec<BackupDescriptor> {
        self.with_storage_lock(|| Ok(list_backup_descriptors(&self.backup_path)))
            .unwrap_or_default()
    }
}
