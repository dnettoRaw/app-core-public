// =============================================================================
//        #######
//     ###       ###     F: storage_backup_io.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0
// =============================================================================

//! Bounded file and manifest I/O for local storage snapshots.

use super::storage_backup::{StorageBackupManifestV1, BACKUP_MANIFEST, MAX_BACKUP_MANIFEST_BYTES};
use super::storage_file_fs::{
    copy_file_bounded, create_new_file, open_regular_file, tmp_path_for, write_atomic_file_using,
};
use super::{StorageError, StorageResult, MAX_STORAGE_BACKUP_FILE_BYTES};
use serde::Deserialize as _;
use sha2::{Digest, Sha256};
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::Path;

const BACKUP_MANIFEST_BUFFER_BYTES: usize = 16 * 1024;

pub(super) fn copy_and_sync(source: &Path, destination: &Path) -> StorageResult<()> {
    let mut source = open_regular_file(source).map_err(|_| StorageError::NotAvailable)?;
    let mut destination = create_new_file(destination).map_err(|_| StorageError::NotAvailable)?;
    copy_file_bounded(&mut source, &mut destination, MAX_STORAGE_BACKUP_FILE_BYTES)
        .map_err(|_| StorageError::NotAvailable)?;
    destination
        .sync_all()
        .map_err(|_| StorageError::NotAvailable)
}

pub(super) fn write_manifest(root: &Path, manifest: &StorageBackupManifestV1) -> StorageResult<()> {
    let path = root.join(BACKUP_MANIFEST);
    write_atomic_file_using(&tmp_path_for(&path), &path, |file| {
        serialize_manifest(file, manifest, MAX_BACKUP_MANIFEST_BYTES)
    })
    .map_err(|_| StorageError::BackupFailed(manifest.name.clone()))
}

pub(super) fn read_manifest(root: &Path, name: &str) -> StorageResult<StorageBackupManifestV1> {
    let path = root.join(BACKUP_MANIFEST);
    let file =
        open_regular_file(&path).map_err(|_| StorageError::BackupFailed(name.to_string()))?;
    let metadata = file
        .metadata()
        .map_err(|_| StorageError::BackupFailed(name.to_string()))?;
    if metadata.len() > MAX_BACKUP_MANIFEST_BYTES {
        return Err(StorageError::BackupFailed(name.to_string()));
    }
    deserialize_manifest(file, metadata.len(), MAX_BACKUP_MANIFEST_BYTES, name)
}

fn serialize_manifest(
    writer: impl Write,
    manifest: &StorageBackupManifestV1,
    max_bytes: u64,
) -> io::Result<()> {
    let mut buffered = BufWriter::with_capacity(BACKUP_MANIFEST_BUFFER_BYTES, writer);
    let mut bounded = BoundedWriter::new(&mut buffered, max_bytes);
    serde_json::to_writer_pretty(&mut bounded, manifest).map_err(io::Error::other)?;
    bounded.flush()
}

fn deserialize_manifest(
    reader: impl Read,
    declared_length: u64,
    max_bytes: u64,
    name: &str,
) -> StorageResult<StorageBackupManifestV1> {
    if declared_length > max_bytes {
        return Err(StorageError::BackupFailed(name.to_string()));
    }
    let read_limit = max_bytes
        .checked_add(1)
        .ok_or_else(|| StorageError::BackupFailed(name.to_string()))?;
    let mut reader =
        BufReader::with_capacity(BACKUP_MANIFEST_BUFFER_BYTES, reader.take(read_limit));
    let mut deserializer = serde_json::Deserializer::from_reader(&mut reader);
    let manifest = StorageBackupManifestV1::deserialize(&mut deserializer)
        .map_err(|_| StorageError::BackupFailed(name.to_string()))?;
    deserializer
        .end()
        .map_err(|_| StorageError::BackupFailed(name.to_string()))?;
    if reader.get_ref().limit() == 0 {
        return Err(StorageError::BackupFailed(name.to_string()));
    }
    Ok(manifest)
}

pub(super) fn hash_file(path: &Path) -> StorageResult<(u64, String)> {
    let mut file = open_regular_file(path).map_err(|_| StorageError::NotAvailable)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 16 * 1024];
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

struct BoundedWriter<W> {
    inner: W,
    remaining: u64,
}

impl<W> BoundedWriter<W> {
    const fn new(inner: W, maximum: u64) -> Self {
        Self {
            inner,
            remaining: maximum,
        }
    }
}

impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let length = bytes.len() as u64;
        if length > self.remaining {
            return Err(io::Error::new(
                io::ErrorKind::FileTooLarge,
                "backup manifest exceeds configured size limit",
            ));
        }
        self.inner.write_all(bytes)?;
        self.remaining -= length;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

#[cfg(test)]
#[path = "storage_backup_io_tests.rs"]
mod tests;
