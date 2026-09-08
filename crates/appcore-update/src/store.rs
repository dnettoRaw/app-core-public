// =============================================================================
//        #######
//     ###       ###     F: store.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 14:12:17 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Defines bounded store contracts and behavior for this crate.

use crate::filesystem::open_regular_file;
use crate::store_io::{
    atomic_write, atomic_write_json, read_json_bounded, reject_directory, remove_if_exists,
    sync_parent_directory, JsonReadError, MAX_UPDATE_METADATA_BYTES,
};
use crate::{sha256_hex, ArtifactDescriptor, UpdateError, UpdateResult};
use appcore_contracts::BuildId;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Stable format version for update pointers and pending activation metadata.
pub const UPDATE_METADATA_FORMAT_VERSION: u16 = 1;
const ARTIFACT_HASH_BUFFER_BYTES: usize = 64 * 1024;

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

#[derive(Serialize)]
struct ArtifactPointerRef<'a> {
    format_version: u16,
    descriptor: &'a ArtifactDescriptor,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingActivationRecord {
    format_version: u16,
    receipt: ActivationReceipt,
}

#[derive(Serialize)]
struct PendingActivationRecordRef<'a> {
    format_version: u16,
    receipt: &'a ActivationReceipt,
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
        let pointer: ArtifactPointer = match read_json_bounded(path, MAX_UPDATE_METADATA_BYTES) {
            Ok(Some(pointer)) => pointer,
            Ok(None) => return Ok(None),
            Err(JsonReadError::Io(error)) => {
                return Err(UpdateError::Store(error.to_string()));
            }
            Err(JsonReadError::Decode(error)) => {
                return Err(UpdateError::Store(error.to_string()));
            }
        };
        if pointer.format_version != UPDATE_METADATA_FORMAT_VERSION {
            return Err(UpdateError::Store(
                "unsupported artifact pointer format".to_string(),
            ));
        }
        pointer.descriptor.validate()?;
        Ok(Some(pointer.descriptor))
    }

    fn write_pointer(&self, path: &Path, descriptor: &ArtifactDescriptor) -> UpdateResult<()> {
        let pointer = ArtifactPointerRef {
            format_version: UPDATE_METADATA_FORMAT_VERSION,
            descriptor,
        };
        atomic_write_json(path, &pointer)
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
        let record: PendingActivationRecord =
            match read_json_bounded(&self.pending_activation(), MAX_UPDATE_METADATA_BYTES) {
                Ok(Some(record)) => record,
                Ok(None) => return Ok(None),
                Err(JsonReadError::Io(error)) => {
                    return Err(UpdateError::Store(error.to_string()));
                }
                Err(JsonReadError::Decode(_)) => {
                    return Err(UpdateError::Store(
                        "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
                    ));
                }
            };
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
        let record = PendingActivationRecordRef {
            format_version: UPDATE_METADATA_FORMAT_VERSION,
            receipt,
        };
        atomic_write_json(&self.pending_activation(), &record)
    }
}

fn verify_artifact(path: &Path, descriptor: &ArtifactDescriptor) -> UpdateResult<()> {
    let mut file =
        open_regular_file(path).map_err(|error| UpdateError::Store(error.to_string()))?;
    if file
        .metadata()
        .map_err(|error| UpdateError::Store(error.to_string()))?
        .len()
        != descriptor.size_bytes()
    {
        return Err(UpdateError::ChecksumMismatch);
    }
    let mut buffer = vec![0_u8; ARTIFACT_HASH_BUFFER_BYTES];
    let mut hasher = Sha256::new();
    let mut size = 0_u64;
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        if read == 0 {
            break;
        }
        size = size
            .checked_add(read as u64)
            .ok_or(UpdateError::ChecksumMismatch)?;
        if size > descriptor.size_bytes() {
            return Err(UpdateError::ChecksumMismatch);
        }
        hasher.update(&buffer[..read]);
    }
    let digest = format!("{:x}", hasher.finalize());
    if size != descriptor.size_bytes() || digest != descriptor.sha256() {
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
