// =============================================================================
//        #######
//     ###       ###     F: cache.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Resumable, hash-addressed artifact cache.

use crate::store_io::{atomic_write_json, remove_if_exists, sync_parent_directory};
use crate::stream::{receive_artifact_from, ArtifactTransferPolicy};
use crate::{
    ArtifactAuthenticityVerifier, ArtifactDescriptor, ArtifactSource, ArtifactWriter, UpdateError,
    UpdateResult,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const CACHE_MAX_DESCRIPTOR_BYTES: usize = 1024 * 1024;

/// Configuration for one bounded update cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CacheOptions {
    /// Maximum aggregate bytes occupied by cache objects, descriptors and parts.
    pub max_bytes: u64,
    /// Applies the current owner-only directory profile where supported.
    pub secure_permissions: bool,
}

impl CacheOptions {
    /// Creates validated cache options.
    pub fn new(max_bytes: u64, secure_permissions: bool) -> UpdateResult<Self> {
        if max_bytes == 0 {
            return Err(UpdateError::Store(
                "cache max_bytes must be greater than zero".to_string(),
            ));
        }
        Ok(Self {
            max_bytes,
            secure_permissions,
        })
    }
}

/// A fully verified artifact published by [`UpdateCache`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedArtifact {
    /// Verified descriptor retained beside the artifact.
    pub descriptor: ArtifactDescriptor,
    /// Immutable hash-addressed artifact path.
    pub artifact_path: PathBuf,
    /// Descriptor metadata path.
    pub descriptor_path: PathBuf,
}

/// Filesystem cache for resumable, opaque artifact downloads.
#[derive(Debug, Clone)]
pub struct UpdateCache {
    root: PathBuf,
    options: CacheOptions,
}

#[derive(Debug, Serialize)]
struct CachedDescriptor<'a> {
    descriptor: &'a ArtifactDescriptor,
}

#[derive(Debug, Deserialize)]
struct StoredDescriptor {
    descriptor: ArtifactDescriptor,
}

impl UpdateCache {
    /// Opens or creates a cache root and its bounded subdirectories.
    pub fn open(root: impl Into<PathBuf>, options: CacheOptions) -> UpdateResult<Self> {
        let cache = Self {
            root: root.into(),
            options,
        };
        cache.initialize()?;
        Ok(cache)
    }

    /// Stages one artifact, resuming a valid partial file when possible.
    pub fn stage(
        &self,
        verifier: &dyn ArtifactAuthenticityVerifier,
        descriptor: &ArtifactDescriptor,
        source: &dyn ArtifactSource,
        transfer: &ArtifactTransferPolicy,
    ) -> UpdateResult<CachedArtifact> {
        descriptor.validate()?;
        verifier.verify(descriptor)?;
        let _lock = self.lock()?;
        self.initialize()?;
        let artifact_path = self.artifact_path(descriptor);
        let descriptor_path = self.descriptor_path(descriptor);
        if self.is_complete(descriptor, &artifact_path, &descriptor_path)? {
            return Ok(CachedArtifact {
                descriptor: descriptor.clone(),
                artifact_path,
                descriptor_path,
            });
        }
        remove_if_exists(&artifact_path)?;
        remove_if_exists(&descriptor_path)?;
        let part_path = self.part_path(descriptor);
        let partial_len = fs::metadata(&part_path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        let result = self.receive_partial(descriptor, source, transfer);
        if matches!(result, Err(UpdateError::ChecksumMismatch)) && partial_len > 0 {
            remove_if_exists(&part_path)?;
            self.receive_partial(descriptor, source, transfer)?;
        } else {
            result?;
        }
        if fs::metadata(&part_path).map_err(store_error)?.len() != descriptor.size_bytes() {
            return Err(UpdateError::ChecksumMismatch);
        }
        fs::rename(&part_path, &artifact_path).map_err(store_error)?;
        sync_parent_directory(&self.objects_dir())?;
        atomic_write_json(&descriptor_path, &CachedDescriptor { descriptor })?;
        Ok(CachedArtifact {
            descriptor: descriptor.clone(),
            artifact_path,
            descriptor_path,
        })
    }

    /// Returns the hash-addressed artifact path for a descriptor.
    pub fn artifact_path(&self, descriptor: &ArtifactDescriptor) -> PathBuf {
        self.objects_dir()
            .join(format!("{}.bin", descriptor.sha256()))
    }

    /// Returns the descriptor metadata path for a descriptor.
    pub fn descriptor_path(&self, descriptor: &ArtifactDescriptor) -> PathBuf {
        self.objects_dir()
            .join(format!("{}.json", descriptor.sha256()))
    }

    /// Returns the resumable partial path for a descriptor.
    pub fn partial_path(&self, descriptor: &ArtifactDescriptor) -> PathBuf {
        self.part_path(descriptor)
    }

    fn initialize(&self) -> UpdateResult<()> {
        let root_created = ensure_directory(&self.root)?;
        let objects = self.objects_dir();
        let parts = self.parts_dir();
        let objects_created = ensure_directory(&objects)?;
        let parts_created = ensure_directory(&parts)?;
        if self.options.secure_permissions {
            if root_created {
                set_private_directory_permissions(&self.root)?;
            }
            if objects_created {
                set_private_directory_permissions(&objects)?;
            }
            if parts_created {
                set_private_directory_permissions(&parts)?;
            }
            validate_private_directory(&self.root)?;
            validate_private_directory(&objects)?;
            validate_private_directory(&parts)?;
            validate_private_ancestors(&self.root)?;
        }
        Ok(())
    }

    fn lock(&self) -> UpdateResult<File> {
        let path = self.root.join("cache.lock");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(UpdateError::Store(
                    "cache lock path is not a regular file".to_string(),
                ));
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(store_error)?;
        file.try_lock_exclusive()
            .map_err(|error| UpdateError::Store(format!("cache lock is unavailable: {error}")))?;
        Ok(file)
    }

    fn is_complete(
        &self,
        descriptor: &ArtifactDescriptor,
        artifact_path: &Path,
        descriptor_path: &Path,
    ) -> UpdateResult<bool> {
        let stored: StoredDescriptor =
            match crate::store_io::read_json_bounded(descriptor_path, CACHE_MAX_DESCRIPTOR_BYTES) {
                Ok(Some(value)) => value,
                Ok(None) => return Ok(false),
                Err(crate::store_io::JsonReadError::Io(error)) => {
                    let _ = remove_if_exists(descriptor_path);
                    return Err(UpdateError::Store(error.to_string()));
                }
                Err(crate::store_io::JsonReadError::Decode(error)) => {
                    let _ = remove_if_exists(descriptor_path);
                    return Err(UpdateError::Store(error.to_string()));
                }
            };
        let artifact_metadata = match fs::symlink_metadata(artifact_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(store_error(error)),
        };
        if stored.descriptor != *descriptor
            || artifact_metadata.file_type().is_symlink()
            || !artifact_metadata.is_file()
        {
            return Ok(false);
        }
        match verify_cached_artifact(artifact_path, descriptor) {
            Ok(()) => {}
            Err(UpdateError::ChecksumMismatch) => {
                remove_if_exists(artifact_path)?;
                remove_if_exists(descriptor_path)?;
                return Ok(false);
            }
            Err(error) => return Err(error),
        }
        Ok(true)
    }

    fn receive_partial(
        &self,
        descriptor: &ArtifactDescriptor,
        source: &dyn ArtifactSource,
        transfer: &ArtifactTransferPolicy,
    ) -> UpdateResult<()> {
        let part_path = self.part_path(descriptor);
        let mut partial = self.open_partial(&part_path, descriptor)?;
        let partial_len = partial.metadata().map_err(store_error)?.len();
        let mut hasher = Sha256::new();
        hash_existing(&mut partial, partial_len, &mut hasher)?;
        let metadata_bytes = serde_json::to_vec(&CachedDescriptor { descriptor })
            .map_err(|error| UpdateError::Store(error.to_string()))?
            .len() as u64;
        let required = descriptor
            .size_bytes()
            .checked_sub(partial_len)
            .and_then(|bytes| bytes.checked_add(metadata_bytes))
            .ok_or(UpdateError::ChecksumMismatch)?;
        if self.usage()?.saturating_add(required) > self.options.max_bytes {
            return Err(UpdateError::Store(
                "update cache quota exceeded".to_string(),
            ));
        }
        let mut writer = PartialWriter { file: partial };
        let result = receive_artifact_from(
            transfer,
            descriptor,
            source,
            &mut writer,
            partial_len,
            hasher,
        );
        let mut partial = writer.file;
        partial.flush().map_err(store_error)?;
        partial.sync_all().map_err(store_error)?;
        result
    }

    fn open_partial(&self, path: &Path, descriptor: &ArtifactDescriptor) -> UpdateResult<File> {
        if let Ok(metadata) = fs::symlink_metadata(path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(UpdateError::Store(
                    "cache partial path is not a regular file".to_string(),
                ));
            }
            if metadata.len() > descriptor.size_bytes() {
                remove_if_exists(path)?;
            }
        }
        OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(path)
            .map_err(store_error)
    }

    fn usage(&self) -> UpdateResult<u64> {
        let mut total = 0_u64;
        for directory in [self.objects_dir(), self.parts_dir()] {
            for entry in fs::read_dir(directory).map_err(store_error)? {
                let entry = entry.map_err(store_error)?;
                let metadata = entry.metadata().map_err(store_error)?;
                if !metadata.is_file() {
                    return Err(UpdateError::Store(
                        "cache contains a non-regular entry".to_string(),
                    ));
                }
                total = total
                    .checked_add(metadata.len())
                    .ok_or_else(|| UpdateError::Store("cache usage overflow".to_string()))?;
            }
        }
        Ok(total)
    }

    fn objects_dir(&self) -> PathBuf {
        self.root.join("objects")
    }

    fn parts_dir(&self) -> PathBuf {
        self.root.join("parts")
    }

    fn part_path(&self, descriptor: &ArtifactDescriptor) -> PathBuf {
        self.parts_dir()
            .join(format!("{}.part", descriptor.sha256()))
    }
}

struct PartialWriter {
    file: File,
}

impl ArtifactWriter for PartialWriter {
    fn write_chunk(&mut self, offset: u64, bytes: &[u8]) -> UpdateResult<()> {
        self.file
            .seek(SeekFrom::Start(offset))
            .and_then(|_| self.file.write_all(bytes))
            .map_err(store_error)
    }
}

fn hash_existing(file: &mut File, length: u64, hasher: &mut Sha256) -> UpdateResult<()> {
    file.seek(SeekFrom::Start(0)).map_err(store_error)?;
    let mut remaining = length;
    let mut buffer = vec![0_u8; 64 * 1024];
    while remaining > 0 {
        let requested = usize::try_from(remaining)
            .unwrap_or(buffer.len())
            .min(buffer.len());
        let read = file.read(&mut buffer[..requested]).map_err(store_error)?;
        if read == 0 {
            return Err(UpdateError::ChecksumMismatch);
        }
        hasher.update(&buffer[..read]);
        remaining -= read as u64;
    }
    Ok(())
}

fn verify_cached_artifact(path: &Path, descriptor: &ArtifactDescriptor) -> UpdateResult<()> {
    let mut file = File::open(path).map_err(store_error)?;
    if file.metadata().map_err(store_error)?.len() != descriptor.size_bytes() {
        return Err(UpdateError::ChecksumMismatch);
    }
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(store_error)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    if crate::integrity::encode_hex(&hasher.finalize()) != descriptor.sha256() {
        return Err(UpdateError::ChecksumMismatch);
    }
    Ok(())
}

fn reject_directory(path: &Path) -> UpdateResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(store_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UpdateError::Store(
            "cache path is not a regular directory".to_string(),
        ));
    }
    Ok(())
}

fn ensure_directory(path: &Path) -> UpdateResult<bool> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(UpdateError::Store(
                    "cache path is not a regular directory".to_string(),
                ));
            }
            Ok(false)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(store_error)?;
            reject_directory(path)?;
            Ok(true)
        }
        Err(error) => Err(store_error(error)),
    }
}

fn store_error(error: std::io::Error) -> UpdateError {
    UpdateError::Store(error.to_string())
}

fn set_private_directory_permissions(path: &Path) -> UpdateResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(store_error)?;
    }
    Ok(())
}

fn validate_private_directory(path: &Path) -> UpdateResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let metadata = fs::symlink_metadata(path).map_err(store_error)?;
        if metadata.file_type().is_symlink()
            || !metadata.is_dir()
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err(UpdateError::Store(
                "cache directory permissions are not owner-only".to_string(),
            ));
        }
    }
    Ok(())
}

fn validate_private_ancestors(path: &Path) -> UpdateResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let mut current = path.to_path_buf();
        while let Some(parent) = current.parent() {
            if parent == current {
                break;
            }
            let metadata = fs::symlink_metadata(parent).map_err(store_error)?;
            let mode = metadata.permissions().mode();
            let safe_sticky_root = mode & 0o1000 != 0 && metadata.uid() == 0;
            if metadata.file_type().is_symlink()
                || !metadata.is_dir()
                || (mode & 0o022 != 0 && !safe_sticky_root)
            {
                return Err(UpdateError::Store(
                    "cache ancestor is writable by an untrusted principal".to_string(),
                ));
            }
            current = parent.to_path_buf();
        }
    }
    Ok(())
}
