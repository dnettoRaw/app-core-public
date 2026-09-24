// =============================================================================
//        #######
//     ###       ###     F: quarantine.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Bounded persistent quarantine for releases that failed health validation.

use crate::store_io::{
    atomic_write_json, read_json_bounded, JsonReadError, MAX_UPDATE_METADATA_BYTES,
};
use crate::{ArtifactDescriptor, UpdateError, UpdateResult};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

/// Version of the persistent quarantine document.
pub const QUARANTINE_FORMAT_VERSION: u16 = 1;
/// Maximum number of release keys accepted by one store.
pub const QUARANTINE_MAX_ENTRIES: usize = 1_024;
const MAX_REASON_BYTES: usize = 512;
const MAX_DOCUMENT_BYTES: usize = 1024 * 1024;

/// Complete release identity used as the quarantine key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct QuarantineKey {
    /// Application identity.
    pub application_id: String,
    /// Release channel.
    pub channel: String,
    /// Semantic application version.
    pub application_version: String,
    /// Immutable build identity.
    pub build_id: String,
    /// Artifact digest.
    pub sha256: String,
}

impl QuarantineKey {
    /// Creates a key from a validated artifact descriptor.
    pub fn from_descriptor(descriptor: &ArtifactDescriptor) -> Self {
        Self {
            application_id: descriptor.application_id().as_str().to_string(),
            channel: descriptor.channel().to_string(),
            application_version: descriptor.application_version().to_string(),
            build_id: descriptor.build_id().as_str().to_string(),
            sha256: descriptor.sha256().to_string(),
        }
    }

    fn validate(&self) -> UpdateResult<()> {
        for (name, value, max) in [
            ("application_id", self.application_id.as_str(), 128),
            ("channel", self.channel.as_str(), 64),
            ("application_version", self.application_version.as_str(), 64),
            ("build_id", self.build_id.as_str(), 128),
            ("sha256", self.sha256.as_str(), 64),
        ] {
            if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
                return Err(UpdateError::Recovery(format!(
                    "quarantine key field {name} is invalid"
                )));
            }
        }
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(UpdateError::Recovery(
                "quarantine key sha256 is invalid".to_string(),
            ));
        }
        Ok(())
    }
}

/// Typed reason why a release was quarantined.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "detail")]
pub enum QuarantineReason {
    /// The activated release failed the deployment health check.
    HealthCheckFailed(String),
    /// Activation failed before the release became healthy.
    ActivationFailed(String),
    /// The artifact bytes did not match the declared digest.
    ChecksumMismatch,
    /// Authenticity or trust validation failed.
    AuthenticityRejected(String),
    /// An operator explicitly required review before reuse.
    ManualReview(String),
}

impl QuarantineReason {
    fn validate(&self) -> UpdateResult<()> {
        let detail = match self {
            Self::HealthCheckFailed(detail)
            | Self::ActivationFailed(detail)
            | Self::AuthenticityRejected(detail)
            | Self::ManualReview(detail) => detail,
            Self::ChecksumMismatch => return Ok(()),
        };
        if detail.trim().is_empty()
            || detail.len() > MAX_REASON_BYTES
            || detail.chars().any(char::is_control)
        {
            return Err(UpdateError::Recovery(
                "quarantine reason is empty, too long or contains control characters".to_string(),
            ));
        }
        Ok(())
    }
}

/// Current state of a quarantine entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineState {
    /// Automatic selection must skip this release.
    Active,
    /// An operator explicitly released this release for selection.
    Released,
}

/// Persisted bounded quarantine entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantineRecord {
    /// Complete release identity.
    pub key: QuarantineKey,
    /// Typed quarantine reason.
    pub reason: QuarantineReason,
    /// First time this key entered quarantine.
    pub quarantined_at_ms: u64,
    /// Last state or reason transition.
    pub updated_at_ms: u64,
    /// Current quarantine state.
    pub state: QuarantineState,
}

impl QuarantineRecord {
    fn validate(&self) -> UpdateResult<()> {
        self.key.validate()?;
        self.reason.validate()?;
        if self.updated_at_ms < self.quarantined_at_ms {
            return Err(UpdateError::Recovery(
                "quarantine transition timestamp precedes creation".to_string(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct QuarantineDocument {
    format_version: u16,
    entries: Vec<QuarantineRecord>,
}

/// Bounded, restart-persistent quarantine store.
#[derive(Debug, Clone)]
pub struct QuarantineStore {
    root: PathBuf,
    max_entries: usize,
}

impl QuarantineStore {
    /// Opens or creates a quarantine store with a bounded entry count.
    pub fn open(root: impl Into<PathBuf>, max_entries: usize) -> UpdateResult<Self> {
        if max_entries == 0 || max_entries > QUARANTINE_MAX_ENTRIES {
            return Err(UpdateError::Recovery(
                "quarantine max_entries is outside the supported bound".to_string(),
            ));
        }
        let store = Self {
            root: root.into(),
            max_entries,
        };
        fs::create_dir_all(&store.root).map_err(store_error)?;
        reject_directory(&store.root)?;
        Ok(store)
    }

    /// Records or reactivates a release quarantine entry.
    pub fn quarantine(
        &self,
        descriptor: &ArtifactDescriptor,
        reason: QuarantineReason,
        at_ms: u64,
    ) -> UpdateResult<QuarantineRecord> {
        descriptor.validate()?;
        reason.validate()?;
        let _lock = self.lock()?;
        let mut document = self.read_document()?;
        let key = QuarantineKey::from_descriptor(descriptor);
        if let Some(entry) = document.entries.iter_mut().find(|entry| entry.key == key) {
            entry.reason = reason;
            entry.updated_at_ms = at_ms;
            entry.state = QuarantineState::Active;
            let result = entry.clone();
            self.write_document(&document)?;
            return Ok(result);
        }
        if document.entries.len() >= self.max_entries {
            return Err(UpdateError::Recovery(
                "quarantine entry bound exceeded; explicit release is required".to_string(),
            ));
        }
        let entry = QuarantineRecord {
            key,
            reason,
            quarantined_at_ms: at_ms,
            updated_at_ms: at_ms,
            state: QuarantineState::Active,
        };
        entry.validate()?;
        document.entries.push(entry.clone());
        self.write_document(&document)?;
        Ok(entry)
    }

    /// Returns whether automatic selection must skip a release key.
    pub fn is_quarantined(&self, key: &QuarantineKey) -> UpdateResult<bool> {
        key.validate()?;
        let _lock = self.lock()?;
        Ok(self
            .read_document()?
            .entries
            .iter()
            .any(|entry| entry.key == *key && entry.state == QuarantineState::Active))
    }

    /// Returns all entries in stable key order without changing state.
    pub fn list(&self) -> UpdateResult<Vec<QuarantineRecord>> {
        let _lock = self.lock()?;
        let mut entries = self.read_document()?.entries;
        entries.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(entries)
    }

    /// Explicitly releases a key while retaining its history across restarts.
    pub fn release(&self, key: &QuarantineKey, at_ms: u64) -> UpdateResult<bool> {
        key.validate()?;
        let _lock = self.lock()?;
        let mut document = self.read_document()?;
        let Some(entry) = document.entries.iter_mut().find(|entry| entry.key == *key) else {
            return Ok(false);
        };
        if entry.state == QuarantineState::Released {
            return Ok(false);
        }
        entry.state = QuarantineState::Released;
        entry.updated_at_ms = at_ms;
        self.write_document(&document)?;
        Ok(true)
    }

    fn read_document(&self) -> UpdateResult<QuarantineDocument> {
        let document = match read_json_bounded::<QuarantineDocument>(
            &self.document_path(),
            MAX_DOCUMENT_BYTES.min(MAX_UPDATE_METADATA_BYTES),
        ) {
            Ok(Some(document)) => document,
            Ok(None) => QuarantineDocument {
                format_version: QUARANTINE_FORMAT_VERSION,
                entries: Vec::new(),
            },
            Err(JsonReadError::Io(error)) => return Err(store_error(error)),
            Err(JsonReadError::Decode(_)) => {
                return Err(UpdateError::Recovery(
                    "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
                ))
            }
        };
        if document.format_version != QUARANTINE_FORMAT_VERSION
            || document.entries.len() > self.max_entries
        {
            return Err(UpdateError::Recovery(
                "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
            ));
        }
        for entry in &document.entries {
            entry.validate()?;
        }
        Ok(document)
    }

    fn write_document(&self, document: &QuarantineDocument) -> UpdateResult<()> {
        atomic_write_json(&self.document_path(), document)
    }

    fn lock(&self) -> UpdateResult<File> {
        let path = self.root.join("quarantine.lock");
        if let Ok(metadata) = fs::symlink_metadata(&path) {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(UpdateError::Recovery(
                    "quarantine lock path is not a regular file".to_string(),
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
        file.try_lock_exclusive().map_err(|error| {
            UpdateError::Recovery(format!("quarantine lock is unavailable: {error}"))
        })?;
        Ok(file)
    }

    fn document_path(&self) -> PathBuf {
        self.root.join("quarantine.json")
    }
}

fn reject_directory(path: &Path) -> UpdateResult<()> {
    let metadata = fs::symlink_metadata(path).map_err(store_error)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UpdateError::Recovery(
            "quarantine root is not a regular directory".to_string(),
        ));
    }
    Ok(())
}

fn store_error(error: std::io::Error) -> UpdateError {
    UpdateError::Recovery(error.to_string())
}
