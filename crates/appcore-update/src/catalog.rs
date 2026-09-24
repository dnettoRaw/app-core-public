// =============================================================================
//        #######
//     ###       ###     F: catalog.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/24 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/24 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Bounded multi-platform release catalog and deterministic selection.

use crate::{
    ArtifactAuthenticityVerifier, ArtifactDescriptor, QuarantineKey, QuarantineStore, UpdateError,
    UpdateResult,
};
use appcore_contracts::ApplicationId;
use semver::Version;
use std::collections::BTreeSet;
use std::io::{BufReader, Read};

/// Maximum encoded catalog size accepted by the built-in reader.
pub const RELEASE_CATALOG_MAX_BYTES: usize = 4 * 1024 * 1024;
/// Maximum number of entries retained by one catalog.
pub const RELEASE_CATALOG_MAX_ENTRIES: usize = 4_096;

/// Local identity used to select one compatible release.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateIdentity {
    /// Application identity.
    pub application_id: ApplicationId,
    /// Target operating system.
    pub os: String,
    /// Target CPU architecture.
    pub architecture: String,
    /// Host installation format.
    pub format: String,
    /// Requested update channel.
    pub channel: String,
    /// Currently installed semantic version.
    pub current_version: String,
}

/// Validated catalog of signed, platform-specific artifact descriptors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReleaseCatalog {
    entries: Vec<ArtifactDescriptor>,
}

impl ReleaseCatalog {
    /// Validates and opens a bounded catalog from JSON.
    pub fn open<R: Read>(
        reader: R,
        verifier: &dyn ArtifactAuthenticityVerifier,
    ) -> UpdateResult<Self> {
        let mut reader = BufReader::new(reader.take(RELEASE_CATALOG_MAX_BYTES as u64 + 1));
        let mut bytes = Vec::with_capacity(RELEASE_CATALOG_MAX_BYTES.min(64 * 1024));
        reader
            .read_to_end(&mut bytes)
            .map_err(|error| UpdateError::Provider(error.to_string()))?;
        if bytes.len() > RELEASE_CATALOG_MAX_BYTES {
            return Err(UpdateError::Provider(
                "release catalog exceeds configured read limit".to_string(),
            ));
        }
        let entries: Vec<ArtifactDescriptor> = serde_json::from_slice(&bytes)
            .map_err(|error| UpdateError::Provider(error.to_string()))?;
        Self::from_entries(entries, verifier)
    }

    /// Validates an owned catalog and all of its signatures.
    pub fn from_entries(
        entries: Vec<ArtifactDescriptor>,
        verifier: &dyn ArtifactAuthenticityVerifier,
    ) -> UpdateResult<Self> {
        if entries.len() > RELEASE_CATALOG_MAX_ENTRIES {
            return Err(UpdateError::Provider(
                "release catalog exceeds entry limit".to_string(),
            ));
        }
        let mut keys = BTreeSet::new();
        for entry in &entries {
            entry.validate()?;
            let target = entry.target().ok_or_else(|| {
                UpdateError::InvalidArtifact(
                    "catalog entries require a platform target".to_string(),
                )
            })?;
            let key = format!(
                "{}\n{}\n{}\n{}\n{}",
                entry.application_id(),
                target.os(),
                target.architecture(),
                target.format(),
                entry.channel(),
            );
            let version_key = format!("{key}\n{}", entry.application_version());
            if !keys.insert(version_key) {
                return Err(UpdateError::InvalidArtifact(
                    "catalog contains ambiguous duplicate release versions".to_string(),
                ));
            }
            verifier.verify(entry)?;
        }
        Ok(Self { entries })
    }

    /// Selects the greatest compatible version above the installed version.
    pub fn select(&self, identity: &UpdateIdentity) -> UpdateResult<Option<ArtifactDescriptor>> {
        self.select_internal(identity, None)
    }

    /// Selects the greatest compatible version that is not actively quarantined.
    pub fn select_with_quarantine(
        &self,
        identity: &UpdateIdentity,
        quarantine: &QuarantineStore,
    ) -> UpdateResult<Option<ArtifactDescriptor>> {
        self.select_internal(identity, Some(quarantine))
    }

    fn select_internal(
        &self,
        identity: &UpdateIdentity,
        quarantine: Option<&QuarantineStore>,
    ) -> UpdateResult<Option<ArtifactDescriptor>> {
        let current = Version::parse(&identity.current_version).map_err(|error| {
            UpdateError::Incompatible(format!("invalid installed version: {error}"))
        })?;
        validate_identity(identity)?;
        let mut selected: Option<(Version, ArtifactDescriptor)> = None;
        for entry in &self.entries {
            let Some(target) = entry.target() else {
                continue;
            };
            if entry.application_id() != &identity.application_id
                || target.os() != identity.os
                || target.architecture() != identity.architecture
                || target.format() != identity.format
                || entry.channel() != identity.channel
            {
                continue;
            }
            if let Some(store) = quarantine {
                if store.is_quarantined(&QuarantineKey::from_descriptor(entry))? {
                    continue;
                }
            }
            let version = Version::parse(entry.application_version()).map_err(|error| {
                UpdateError::InvalidArtifact(format!("invalid catalog version: {error}"))
            })?;
            if version <= current || selected.as_ref().is_some_and(|(best, _)| version <= *best) {
                continue;
            }
            selected = Some((version, entry.clone()));
        }
        Ok(selected.map(|(_, entry)| entry))
    }

    /// Returns the number of validated catalog entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the catalog has no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

fn validate_identity(identity: &UpdateIdentity) -> UpdateResult<()> {
    for (name, value) in [
        ("os", identity.os.as_str()),
        ("architecture", identity.architecture.as_str()),
        ("format", identity.format.as_str()),
        ("channel", identity.channel.as_str()),
    ] {
        if value.trim().is_empty() || value.len() > 64 || value.chars().any(char::is_control) {
            return Err(UpdateError::Incompatible(format!(
                "update identity {name} is invalid"
            )));
        }
    }
    Ok(())
}
