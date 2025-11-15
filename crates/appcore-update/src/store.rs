// =============================================================================
//        #######
//     ###       ###     F: store.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 14:12:17 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::filesystem::read_regular_file_bounded;
use crate::{sha256_hex, ArtifactDescriptor, UpdateError, UpdateResult};
use appcore_contracts::BuildId;
use serde::{Deserialize, Serialize};
#[cfg(unix)]
use std::fs::File;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Stable format version for update pointers and pending activation metadata.
pub const UPDATE_METADATA_FORMAT_VERSION: u16 = 1;
// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static UPDATE_TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Opaque staged artifact owned by an artifact store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedArtifact {
    /// Descriptor staged for activation.
    pub descriptor: ArtifactDescriptor,
    /// Store-owned staging reference.
    pub staging_reference: String,
}

/// Receipt required to commit or roll back one activation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivationReceipt {
    /// Artifact that was activated.
    pub activated: ArtifactDescriptor,
    /// Previously active artifact, when one existed.
    pub previous: Option<ArtifactDescriptor>,
}

/// Store contract for atomic staging and reversible activation.
pub trait ArtifactStore: Send + Sync {
    /// Recovers an activation interrupted before commit.
    ///
    /// Stores without durable activation metadata may keep the default no-op.
    fn recover(&self) -> UpdateResult<()> {
        Ok(())
    }
    /// Returns the currently active artifact.
    fn current(&self) -> UpdateResult<Option<ArtifactDescriptor>>;
    /// Persists verified bytes without changing the active artifact.
    fn stage(&self, descriptor: &ArtifactDescriptor, bytes: &[u8]) -> UpdateResult<StagedArtifact>;
    /// Removes a staged artifact that failed a pre-activation smoke test.
    fn discard_staged(&self, _staged: &StagedArtifact) -> UpdateResult<()> {
        Ok(())
    }
    /// Atomically makes a staged artifact active and returns rollback state.
    fn activate(&self, staged: StagedArtifact) -> UpdateResult<ActivationReceipt>;
    /// Restores the previous artifact from an activation receipt.
    fn rollback(&self, receipt: &ActivationReceipt) -> UpdateResult<()>;
    /// Finalizes a healthy activation and discards rollback metadata.
    fn commit(&self, receipt: &ActivationReceipt) -> UpdateResult<()>;
}

/// Filesystem artifact store using atomic pointer replacement.
#[derive(Debug, Clone)]
pub struct FileArtifactStore {
    root: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ArtifactPointer {
    format_version: u16,
    descriptor: ArtifactDescriptor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingActivationRecord {
    format_version: u16,
    receipt: ActivationReceipt,
}

impl FileArtifactStore {
    /// Creates a store rooted at an installation-owned directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Returns the artifact file retained for a build.
    pub fn artifact_path(&self, build_id: &BuildId) -> PathBuf {
        self.root
            .join("artifacts")
            .join(format!("{}.artifact", build_id.as_str()))
    }

    /// Returns the private path for a staged artifact.
    pub fn staged_artifact_path(&self, staged: &StagedArtifact) -> PathBuf {
        self.staged_path(staged.descriptor.build_id())
    }

    /// Returns a durable activation awaiting supervisor health verification.
    pub fn pending_activation_receipt(&self) -> UpdateResult<Option<ActivationReceipt>> {
        self.read_pending_activation()
    }

    fn staged_path(&self, build_id: &BuildId) -> PathBuf {
        self.root
            .join("staged")
            .join(format!("{}.artifact", build_id.as_str()))
    }

    fn active_pointer(&self) -> PathBuf {
        self.root.join("active.json")
    }

    fn previous_pointer(&self) -> PathBuf {
        self.root.join("previous.json")
    }

    fn pending_activation(&self) -> PathBuf {
        self.root.join("pending-activation.json")
    }

    fn initialize(&self) -> UpdateResult<()> {
        fs::create_dir_all(self.root.join("artifacts"))
            .and_then(|_| fs::create_dir_all(self.root.join("staged")))
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        reject_directory(&self.root)?;
        reject_directory(&self.root.join("artifacts"))?;
        reject_directory(&self.root.join("staged"))
    }

    fn read_pointer(&self, path: &Path) -> UpdateResult<Option<ArtifactDescriptor>> {
        let bytes = match read_regular_file_bounded(path, 1_048_576) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(UpdateError::Store(error.to_string())),
        };
        let pointer: ArtifactPointer = serde_json::from_slice(&bytes)
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        if pointer.format_version != UPDATE_METADATA_FORMAT_VERSION {
            return Err(UpdateError::Store(
                "unsupported artifact pointer format".to_string(),
            ));
        }
        pointer.descriptor.validate()?;
        Ok(Some(pointer.descriptor))
    }

    fn write_pointer(&self, path: &Path, descriptor: &ArtifactDescriptor) -> UpdateResult<()> {
        let pointer = ArtifactPointer {
            format_version: UPDATE_METADATA_FORMAT_VERSION,
            descriptor: descriptor.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&pointer)
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        atomic_write(path, &bytes)
    }
}

impl ArtifactStore for FileArtifactStore {
    fn recover(&self) -> UpdateResult<()> {
        let Some(receipt) = self.read_pending_activation()? else {
            remove_if_exists(&self.previous_pointer())?;
            return Ok(());
        };
        match self.current()? {
            Some(current) if current.build_id() == receipt.activated.build_id() => {
                self.rollback(&receipt)
            }
            _ => {
                remove_if_exists(&self.previous_pointer())?;
                remove_if_exists(&self.pending_activation())
            }
        }
    }

    fn current(&self) -> UpdateResult<Option<ArtifactDescriptor>> {
        self.read_pointer(&self.active_pointer())
    }

    fn stage(&self, descriptor: &ArtifactDescriptor, bytes: &[u8]) -> UpdateResult<StagedArtifact> {
        self.initialize()?;
        if bytes.len() as u64 != descriptor.size_bytes() || sha256_hex(bytes) != descriptor.sha256()
        {
            return Err(UpdateError::ChecksumMismatch);
        }
        let path = self.staged_path(descriptor.build_id());
        atomic_write(&path, bytes)?;
        Ok(StagedArtifact {
            descriptor: descriptor.clone(),
            staging_reference: path.to_string_lossy().into_owned(),
        })
    }

    fn discard_staged(&self, staged: &StagedArtifact) -> UpdateResult<()> {
        let expected = self.staged_path(staged.descriptor.build_id());
        if staged.staging_reference != expected.to_string_lossy() {
            return Err(UpdateError::Store(
                "staged artifact reference does not belong to this store".to_string(),
            ));
        }
        remove_if_exists(&expected)
    }

    fn activate(&self, staged: StagedArtifact) -> UpdateResult<ActivationReceipt> {
        self.activate_inner(staged, None)
    }

    fn rollback(&self, receipt: &ActivationReceipt) -> UpdateResult<()> {
        let current = self.current()?.ok_or_else(|| {
            UpdateError::Store("cannot rollback without an active artifact".to_string())
        })?;
        if current.build_id() != receipt.activated.build_id() {
            return Err(UpdateError::Store(
                "active artifact changed after activation".to_string(),
            ));
        }
        match &receipt.previous {
            Some(previous) => self.write_pointer(&self.active_pointer(), previous)?,
            None => remove_if_exists(&self.active_pointer())?,
        }
        remove_if_exists(&self.previous_pointer())?;
        remove_if_exists(&self.pending_activation())
    }

    fn commit(&self, receipt: &ActivationReceipt) -> UpdateResult<()> {
        let current = self.current()?.ok_or_else(|| {
            UpdateError::Store("cannot commit without an active artifact".to_string())
        })?;
        if current.build_id() != receipt.activated.build_id() {
            return Err(UpdateError::Store(
                "active artifact changed before commit".to_string(),
            ));
        }
        remove_if_exists(&self.previous_pointer())?;
        remove_if_exists(&self.pending_activation())
    }
}

#[cfg_attr(not(test), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StoreFaultPoint {
    ArtifactMoved,
    PreviousPointerWritten,
    PendingReceiptWritten,
    ActivePointerWritten,
}

impl FileArtifactStore {
    fn activate_inner(
        &self,
        staged: StagedArtifact,
        fault: Option<StoreFaultPoint>,
    ) -> UpdateResult<ActivationReceipt> {
        self.initialize()?;
        let expected = self.staged_path(staged.descriptor.build_id());
        if Path::new(&staged.staging_reference) != expected {
            return Err(UpdateError::Store(
                "staging reference does not belong to this store".to_string(),
            ));
        }
        let previous = self.current()?;
        let artifact_path = self.artifact_path(staged.descriptor.build_id());
        verify_artifact(&expected, &staged.descriptor)?;
        install_artifact(&expected, &artifact_path, &staged.descriptor)?;
        sync_parent_directory(
            artifact_path
                .parent()
                .ok_or_else(|| UpdateError::Store("artifact path has no parent".to_string()))?,
        )?;
        inject_store_fault(fault, StoreFaultPoint::ArtifactMoved)?;
        if let Some(previous) = &previous {
            self.write_pointer(&self.previous_pointer(), previous)?;
        } else {
            remove_if_exists(&self.previous_pointer())?;
        }
        inject_store_fault(fault, StoreFaultPoint::PreviousPointerWritten)?;
        let receipt = ActivationReceipt {
            activated: staged.descriptor.clone(),
            previous,
        };
        self.write_pending_activation(&receipt)?;
        inject_store_fault(fault, StoreFaultPoint::PendingReceiptWritten)?;
        self.write_pointer(&self.active_pointer(), &staged.descriptor)?;
        inject_store_fault(fault, StoreFaultPoint::ActivePointerWritten)?;
        Ok(receipt)
    }

    #[cfg(test)]
    pub(crate) fn activate_with_fault(
        &self,
        staged: StagedArtifact,
        fault: StoreFaultPoint,
    ) -> UpdateResult<ActivationReceipt> {
        self.activate_inner(staged, Some(fault))
    }

    fn read_pending_activation(&self) -> UpdateResult<Option<ActivationReceipt>> {
        let bytes = match read_bounded(&self.pending_activation(), 1_048_576)? {
            Some(bytes) => bytes,
            None => return Ok(None),
        };
        let record = serde_json::from_slice::<PendingActivationRecord>(&bytes)
            .map_err(|_| UpdateError::Store("NO MORE SUPPORTED PLEASE UPDATE".to_string()))?;
        if record.format_version != UPDATE_METADATA_FORMAT_VERSION {
            return Err(UpdateError::Store(
                "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
            ));
        }
        let receipt = record.receipt;
        receipt.activated.validate()?;
        if let Some(previous) = &receipt.previous {
            previous.validate()?;
        }
        Ok(Some(receipt))
    }

    fn write_pending_activation(&self, receipt: &ActivationReceipt) -> UpdateResult<()> {
        let record = PendingActivationRecord {
            format_version: UPDATE_METADATA_FORMAT_VERSION,
            receipt: receipt.clone(),
        };
        let bytes = serde_json::to_vec_pretty(&record)
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        atomic_write(&self.pending_activation(), &bytes)
    }
}

fn read_bounded(path: &Path, max_bytes: usize) -> UpdateResult<Option<Vec<u8>>> {
    let bytes = match read_regular_file_bounded(path, max_bytes) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(UpdateError::Store(error.to_string())),
    };
    Ok(Some(bytes))
}

fn verify_artifact(path: &Path, descriptor: &ArtifactDescriptor) -> UpdateResult<()> {
    let max_bytes = usize::try_from(descriptor.size_bytes())
        .map_err(|_| UpdateError::Store("artifact size exceeds this platform".to_string()))?;
    let bytes = read_regular_file_bounded(path, max_bytes)
        .map_err(|error| UpdateError::Store(error.to_string()))?;
    if bytes.len() as u64 != descriptor.size_bytes() || sha256_hex(&bytes) != descriptor.sha256() {
        return Err(UpdateError::ChecksumMismatch);
    }
    Ok(())
}

fn install_artifact(
    staged_path: &Path,
    artifact_path: &Path,
    descriptor: &ArtifactDescriptor,
) -> UpdateResult<()> {
    match fs::hard_link(staged_path, artifact_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            verify_artifact(artifact_path, descriptor)?;
        }
        Err(error) => return Err(UpdateError::Store(error.to_string())),
    }
    remove_if_exists(staged_path)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> UpdateResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| UpdateError::Store("path has no parent".to_string()))?;
    fs::create_dir_all(parent).map_err(|error| UpdateError::Store(error.to_string()))?;
    reject_directory(parent)?;
    reject_optional_regular_file(path)?;
    let temporary = path.with_extension(format!(
        "tmp-{}-{}",
        std::process::id(),
        UPDATE_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        fs::rename(&temporary, path).map_err(|error| UpdateError::Store(error.to_string()))?;
        sync_parent_directory(parent)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(unix)]
fn sync_parent_directory(path: &Path) -> UpdateResult<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| UpdateError::Store(error.to_string()))
}

#[cfg(not(unix))]
fn sync_parent_directory(_path: &Path) -> UpdateResult<()> {
    Ok(())
}

fn remove_if_exists(path: &Path) -> UpdateResult<()> {
    match fs::remove_file(path) {
        Ok(()) => path
            .parent()
            .map(sync_parent_directory)
            .transpose()
            .map(|_| ()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateError::Store(error.to_string())),
    }
}

fn inject_store_fault(
    actual: Option<StoreFaultPoint>,
    expected: StoreFaultPoint,
) -> UpdateResult<()> {
    if actual == Some(expected) {
        return Err(UpdateError::Store(format!(
            "injected store fault at {expected:?}"
        )));
    }
    Ok(())
}

fn reject_optional_regular_file(path: &Path) -> UpdateResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => Err(
            UpdateError::Store("update path is not a regular file".to_string()),
        ),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UpdateError::Store(error.to_string())),
    }
}

fn reject_directory(path: &Path) -> UpdateResult<()> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| UpdateError::Store(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UpdateError::Store(
            "update root is not a regular directory".to_string(),
        ));
    }
    Ok(())
}
