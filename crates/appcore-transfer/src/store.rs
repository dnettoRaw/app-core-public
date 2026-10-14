// =============================================================================
//        #######
//     ###       ###     F: store.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Locked, quota-accounted durable blocks and immutable object publication.
use crate::{
    descriptor::{hex, valid_hash},
    filesystem as io, ObjectDescriptor, TransferControl, TransferError as E, TransferLimits,
    TransferResult,
};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
/// One exclusive process-local store owner. The file lock is held until drop.
/// Existing ancestors must be deployment-protected; Windows additionally requires
/// a deployment-owned private ACL. Reparse points are rejected on all Windows paths.
#[derive(Debug)]
pub struct TransferStore {
    root: PathBuf,
    limits: TransferLimits,
    _lock: File,
    #[cfg(windows)]
    _root_guard: appcore_security::PrivateDirectoryGuard,
    used: u64,
    files: usize,
}
#[derive(Serialize)]
struct Metadata {
    format_version: u16,
    created_ms: u64,
    descriptor: ObjectDescriptor,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MetadataWire {
    format_version: u16,
    created_ms: u64,
    descriptor: crate::descriptor::DescriptorWire,
}
/// An immutable, fully hash-verified object. This is not an authenticated release.
#[derive(Debug, Clone)]
pub struct VerifiedObject {
    path: PathBuf,
    descriptor: ObjectDescriptor,
}
impl VerifiedObject {
    /// Immutable content path; no automatic installation or activation.
    pub fn path(&self) -> &Path {
        &self.path
    }
    /// Verified immutable object descriptor.
    pub fn descriptor(&self) -> &ObjectDescriptor {
        &self.descriptor
    }
    /// Rechecks complete content before returning a fresh reader; callers own their I/O deadline.
    pub fn open(&self) -> TransferResult<File> {
        verify_complete(&self.path, &self.descriptor)?;
        io::open(&self.path)
    }
}
/// Exclusive mutable resumable state for one object.
pub struct TransferSession<'a> {
    store: &'a mut TransferStore,
    dir: PathBuf,
    descriptor: ObjectDescriptor,
    complete: Vec<bool>,
    #[cfg(windows)]
    _directory_guard: appcore_security::PrivateDirectoryGuard,
}
impl TransferStore {
    /// Opens an existing owner-protected root, holds a nonblocking cross-process lock,
    /// and scans quota before admitting new work. Never creates arbitrary ancestors.
    pub fn open(root: impl Into<PathBuf>, limits: TransferLimits) -> TransferResult<Self> {
        limits.validate()?;
        let root = root.into();
        io::validate_root(&root)?;
        #[cfg(windows)]
        let root_guard =
            appcore_security::create_update_private_directory(&root).map_err(|_| E::UnsafePath)?;
        let path = root.join(".lock");
        let lock = if path.try_exists().map_err(|_| E::Storage)? {
            io::open(&path)?
        } else {
            io::exclusive(&path)?
        };
        lock.try_lock_exclusive().map_err(|_| E::Busy)?;
        let mut value = Self {
            root,
            limits,
            _lock: lock,
            #[cfg(windows)]
            _root_guard: root_guard,
            used: 0,
            files: 0,
        };
        value.recount()?;
        Ok(value)
    }
    /// Starts/reopens only this exact authenticated-by-caller descriptor. Recovered
    /// block hashes are revalidated. Corrupt metadata or blocks fail closed and stay
    /// available for diagnostics; purge the partial explicitly before redownloading.
    pub fn begin(&mut self, descriptor: ObjectDescriptor) -> TransferResult<TransferSession<'_>> {
        descriptor.validate(self.limits)?;
        let dir = self.root.join(descriptor.object_hash());
        if !dir.try_exists().map_err(|_| E::Storage)? {
            self.reserve(0, 1)?;
            io::make_directory(&dir)?;
            self.recount()?;
        }
        io::private_directory(&dir)?;
        #[cfg(windows)]
        let directory_guard =
            appcore_security::create_update_private_directory(&dir).map_err(|_| E::UnsafePath)?;
        let metadata_path = dir.join("descriptor.json");
        if metadata_path.try_exists().map_err(|_| E::Storage)? {
            let metadata = read_metadata(&metadata_path, self.limits)?;
            if metadata.descriptor != descriptor {
                return Err(E::Integrity);
            }
        } else {
            for entry in fs::read_dir(&dir).map_err(|_| E::Storage)? {
                let entry = entry.map_err(|_| E::Storage)?;
                io::regular(&entry.path())?;
                if entry.file_name() != "descriptor.tmp" {
                    return Err(E::Integrity);
                }
                fs::remove_file(entry.path()).map_err(|_| E::Storage)?;
            }
            self.recount()?;
            let metadata = Metadata {
                format_version: 1,
                created_ms: now_ms()?,
                descriptor: descriptor.clone(),
            };
            let bytes = serde_json::to_vec(&metadata).map_err(|_| E::Storage)?;
            self.reserve(bytes.len() as u64, 2)?;
            io::write(&metadata_path, &bytes)?;
            self.recount()?;
        }
        cleanup_temporaries(&dir, &descriptor)?;
        self.recount()?;
        let mut complete = vec![false; descriptor.block_count()];
        let final_path = dir.join("complete.bin");
        if final_path.try_exists().map_err(|_| E::Storage)? {
            verify_complete(&final_path, &descriptor)?;
            complete.fill(true);
        } else {
            for (index, done) in complete.iter_mut().enumerate() {
                let path = block_path(&dir, index);
                if path.try_exists().map_err(|_| E::Storage)? {
                    let bytes = io::read(&path, descriptor.block_len(index)? as u64)?;
                    descriptor.verify_block(index, &bytes)?;
                    *done = true;
                }
            }
        }
        Ok(TransferSession {
            store: self,
            dir,
            descriptor,
            complete,
            #[cfg(windows)]
            _directory_guard: directory_guard,
        })
    }
    /// Explicit TTL cleanup of incomplete objects only, under the root lock.
    /// Unknown/corrupt metadata fails closed. Complete objects are never removed.
    pub fn prune_expired(&mut self, ttl: Duration) -> TransferResult<usize> {
        if ttl.is_zero() || ttl > Duration::from_secs(365 * 86400) {
            return Err(E::InvalidContract);
        }
        let now = now_ms()?;
        let mut removed = 0;
        for entry in fs::read_dir(&self.root).map_err(|_| E::Storage)? {
            let entry = entry.map_err(|_| E::Storage)?;
            if entry.file_name() == ".lock" {
                continue;
            }
            let dir = entry.path();
            io::private_directory(&dir)?;
            let m = read_metadata(&dir.join("descriptor.json"), self.limits)?;
            if dir.file_name().and_then(|name| name.to_str()) != Some(m.descriptor.object_hash()) {
                return Err(E::Integrity);
            }
            validate_inventory(&dir, &m.descriptor)?;
            if !dir
                .join("complete.bin")
                .try_exists()
                .map_err(|_| E::Storage)?
                && now.saturating_sub(m.created_ms) >= ttl.as_millis() as u64
            {
                fs::remove_dir_all(&dir).map_err(|_| E::Storage)?;
                io::sync_parent(&dir)?;
                removed += 1;
            }
        }
        self.recount()?;
        Ok(removed)
    }
    /// Explicitly removes only a known incomplete object's private inventory.
    /// A complete object, unknown file or invalid metadata prevents deletion.
    pub fn purge_partial(&mut self, descriptor: &ObjectDescriptor) -> TransferResult<()> {
        descriptor.validate(self.limits)?;
        let dir = self.root.join(descriptor.object_hash());
        io::private_directory(&dir)?;
        if read_metadata(&dir.join("descriptor.json"), self.limits)?.descriptor != *descriptor {
            return Err(E::Integrity);
        }
        if dir
            .join("complete.bin")
            .try_exists()
            .map_err(|_| E::Storage)?
        {
            return Err(E::InvalidContract);
        }
        validate_inventory(&dir, descriptor)?;
        fs::remove_dir_all(&dir).map_err(|_| E::Storage)?;
        io::sync_parent(&dir)?;
        self.recount()
    }
    fn reserve(&self, bytes: u64, files: usize) -> TransferResult<()> {
        if self
            .used
            .checked_add(bytes)
            .is_none_or(|n| n > self.limits.max_store_bytes)
            || self
                .files
                .checked_add(files)
                .is_none_or(|n| n > self.limits.max_files)
        {
            return Err(E::Limit);
        }
        Ok(())
    }
    fn recount(&mut self) -> TransferResult<()> {
        let mut bytes = 0u64;
        let mut count = 0usize;
        for entry in fs::read_dir(&self.root).map_err(|_| E::Storage)? {
            let entry = entry.map_err(|_| E::Storage)?;
            count += 1;
            if count > self.limits.max_files {
                return Err(E::Limit);
            }
            if entry.file_name() == ".lock" {
                bytes = bytes
                    .checked_add(io::regular(&entry.path())?.len())
                    .ok_or(E::Limit)?;
                continue;
            }
            if !entry.file_name().to_str().is_some_and(valid_hash) {
                return Err(E::UnsafePath);
            }
            io::private_directory(&entry.path())?;
            for file in fs::read_dir(entry.path()).map_err(|_| E::Storage)? {
                let file = file.map_err(|_| E::Storage)?;
                count += 1;
                if count > self.limits.max_files {
                    return Err(E::Limit);
                }
                bytes = bytes
                    .checked_add(io::regular(&file.path())?.len())
                    .ok_or(E::Limit)?;
                if bytes > self.limits.max_store_bytes {
                    return Err(E::Limit);
                }
            }
        }
        if bytes > self.limits.max_store_bytes {
            return Err(E::Limit);
        }
        self.used = bytes;
        self.files = count;
        Ok(())
    }
}
impl TransferSession<'_> {
    /// Exact admitted descriptor.
    pub fn descriptor(&self) -> &ObjectDescriptor {
        &self.descriptor
    }
    /// Snapshot of missing block indices; bounded by max_blocks.
    pub fn missing(&self) -> Vec<usize> {
        self.complete
            .iter()
            .enumerate()
            .filter_map(|(i, done)| (!done).then_some(i))
            .collect()
    }
    /// Verifies a block before exclusively staging, syncing and atomically publishing.
    /// An identical persisted block is an idempotent no-op.
    pub fn accept(&mut self, index: usize, bytes: &[u8]) -> TransferResult<()> {
        self.descriptor.verify_block(index, bytes)?;
        if self.complete[index] {
            return Ok(());
        }
        self.store.reserve(bytes.len() as u64, 2)?;
        let result = io::write(&block_path(&self.dir, index), bytes);
        self.store.recount()?;
        result?;
        self.complete[index] = true;
        Ok(())
    }
    /// Publishes only after every block and full-object digest match. Failure never
    /// marks incomplete content as complete. A post-rename directory-sync error may
    /// leave complete bytes visible; retry/reopen revalidates before returning them.
    pub fn finish(self) -> TransferResult<VerifiedObject> {
        self.finish_inner(None)
    }
    /// Cooperatively checks cancellation between bounded assembly/hash operations.
    /// Filesystem calls still require deployment-owned I/O bounds.
    pub fn finish_with_control(self, control: &TransferControl) -> TransferResult<VerifiedObject> {
        self.finish_inner(Some(control))
    }
    fn finish_inner(self, control: Option<&TransferControl>) -> TransferResult<VerifiedObject> {
        checkpoint(control)?;
        if self.complete.iter().any(|done| !*done) {
            return Err(E::Integrity);
        }
        let path = self.dir.join("complete.bin");
        if !path.try_exists().map_err(|_| E::Storage)? {
            self.store.reserve(self.descriptor.total_bytes(), 1)?;
            let result = io::atomic(&path, |file| {
                assemble(file, &self.dir, &self.descriptor, control)
            });
            self.store.recount()?;
            result?;
        }
        verify_complete_controlled(&path, &self.descriptor, control)?;
        for index in 0..self.descriptor.block_count() {
            let block = block_path(&self.dir, index);
            if block.try_exists().map_err(|_| E::Storage)? {
                io::regular(&block)?;
                fs::remove_file(block).map_err(|_| E::Storage)?;
            }
        }
        io::sync_parent(&path)?;
        self.store.recount()?;
        Ok(VerifiedObject {
            path,
            descriptor: self.descriptor,
        })
    }
}
fn assemble(
    file: &mut File,
    dir: &Path,
    descriptor: &ObjectDescriptor,
    control: Option<&TransferControl>,
) -> TransferResult<()> {
    let mut full = Sha256::new();
    for index in 0..descriptor.block_count() {
        checkpoint(control)?;
        let bytes = io::read(&block_path(dir, index), descriptor.block_len(index)? as u64)?;
        descriptor.verify_block(index, &bytes)?;
        file.write_all(&bytes).map_err(|_| E::Storage)?;
        full.update(&bytes);
    }
    checkpoint(control)?;
    if hex(&full.finalize()) != descriptor.object_hash() {
        return Err(E::Integrity);
    }
    Ok(())
}
fn checkpoint(control: Option<&TransferControl>) -> TransferResult<()> {
    control.map_or(Ok(()), TransferControl::checkpoint)
}
fn verify_complete(path: &Path, descriptor: &ObjectDescriptor) -> TransferResult<()> {
    verify_complete_controlled(path, descriptor, None)
}
fn verify_complete_controlled(
    path: &Path,
    descriptor: &ObjectDescriptor,
    control: Option<&TransferControl>,
) -> TransferResult<()> {
    let mut file = io::open(path)?;
    if file.metadata().map_err(|_| E::Storage)?.len() != descriptor.total_bytes() {
        return Err(E::Integrity);
    }
    let mut hasher = Sha256::new();
    let mut buffer = [0; 64 * 1024];
    loop {
        checkpoint(control)?;
        let n = file.read(&mut buffer).map_err(|_| E::Storage)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    if hex(&hasher.finalize()) != descriptor.object_hash() {
        return Err(E::Integrity);
    }
    Ok(())
}
fn block_path(dir: &Path, index: usize) -> PathBuf {
    dir.join(format!("{index:04}.block"))
}
fn read_metadata(path: &Path, limits: TransferLimits) -> TransferResult<Metadata> {
    let bytes = io::read(path, 1024 * 1024)?;
    let wire: MetadataWire = serde_json::from_slice(&bytes).map_err(|_| E::Integrity)?;
    if wire.format_version != 1 {
        return Err(E::UnsupportedVersion);
    }
    let m = Metadata {
        format_version: wire.format_version,
        created_ms: wire.created_ms,
        descriptor: ObjectDescriptor::try_from(wire.descriptor)?,
    };
    if m.format_version != 1 {
        return Err(E::UnsupportedVersion);
    }
    m.descriptor.validate(limits)?;
    Ok(m)
}
fn validate_inventory(dir: &Path, descriptor: &ObjectDescriptor) -> TransferResult<()> {
    for entry in fs::read_dir(dir).map_err(|_| E::Storage)? {
        let entry = entry.map_err(|_| E::Storage)?;
        io::regular(&entry.path())?;
        let name = entry.file_name();
        let name = name.to_str().ok_or(E::UnsafePath)?;
        let valid = name == "descriptor.json"
            || name == "descriptor.tmp"
            || name == "complete.bin"
            || name == "complete.tmp"
            || (0..descriptor.block_count())
                .any(|i| name == format!("{i:04}.block") || name == format!("{i:04}.tmp"));
        if !valid {
            return Err(E::UnsafePath);
        }
    }
    Ok(())
}
fn cleanup_temporaries(dir: &Path, descriptor: &ObjectDescriptor) -> TransferResult<()> {
    validate_inventory(dir, descriptor)?;
    for entry in fs::read_dir(dir).map_err(|_| E::Storage)? {
        let entry = entry.map_err(|_| E::Storage)?;
        if entry.path().extension().is_some_and(|e| e == "tmp") {
            fs::remove_file(entry.path()).map_err(|_| E::Storage)?;
        }
    }
    io::sync_parent(&dir.join("descriptor.json"))
}
fn now_ms() -> TransferResult<u64> {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| E::Storage)?
            .as_millis(),
    )
    .map_err(|_| E::Storage)
}
