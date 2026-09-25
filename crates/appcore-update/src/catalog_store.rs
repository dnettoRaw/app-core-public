// =============================================================================
//        #######
//     ###       ###     F: catalog_store.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/25 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/25 00:00:00 by dnettoRaw
//      ###########      S: 1.0.3-rc
// =============================================================================

//! Bounded release catalog with safe local artifact locations.

use crate::catalog::{ReleaseCatalog, RELEASE_CATALOG_MAX_BYTES, RELEASE_CATALOG_MAX_ENTRIES};
use crate::filesystem::{open_regular_file, read_regular_file_bounded};
use crate::stream::{ArtifactSource, DEFAULT_ARTIFACT_CHUNK_BYTES};
use crate::{ArtifactAuthenticityVerifier, ArtifactDescriptor, UpdateError, UpdateResult};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Component, Path, PathBuf};

/// One signed descriptor and its location relative to the catalog root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    /// Signed immutable artifact descriptor.
    pub descriptor: ArtifactDescriptor,
    /// Safe relative path containing the immutable artifact bytes.
    pub relative_path: String,
}

/// Persistent catalog that serves bytes only for catalog-owned descriptors.
#[derive(Debug, Clone)]
pub struct ReleaseCatalogStore {
    root: PathBuf,
    catalog: ReleaseCatalog,
    entries: BTreeMap<String, CatalogEntry>,
}

impl ReleaseCatalogStore {
    /// Opens and validates a catalog below an existing controlled directory.
    pub fn open(
        root: impl AsRef<Path>,
        catalog_path: impl AsRef<Path>,
        verifier: &dyn ArtifactAuthenticityVerifier,
    ) -> UpdateResult<Self> {
        let root = canonical_directory(root.as_ref())?;
        let catalog_path = resolve_safe_path(&root, catalog_path.as_ref())?;
        let bytes = read_regular_file_bounded(&catalog_path, RELEASE_CATALOG_MAX_BYTES)
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        let entries: Vec<CatalogEntry> = serde_json::from_slice(&bytes)
            .map_err(|error| UpdateError::Store(error.to_string()))?;
        if entries.len() > RELEASE_CATALOG_MAX_ENTRIES {
            return Err(UpdateError::Store(
                "release catalog exceeds entry limit".to_string(),
            ));
        }
        let descriptors = entries
            .iter()
            .map(|entry| {
                validate_relative_path(&entry.relative_path)?;
                resolve_safe_path(&root, Path::new(&entry.relative_path))?;
                Ok(entry.descriptor.clone())
            })
            .collect::<UpdateResult<Vec<_>>>()?;
        let catalog = ReleaseCatalog::from_entries(descriptors, verifier)?;
        let mut indexed = BTreeMap::new();
        for entry in entries {
            if indexed
                .insert(entry.descriptor.sha256().to_string(), entry)
                .is_some()
            {
                return Err(UpdateError::InvalidArtifact(
                    "catalog contains ambiguous duplicate artifact hashes".to_string(),
                ));
            }
        }
        Ok(Self {
            root,
            catalog,
            entries: indexed,
        })
    }

    /// Returns the validated release catalog for version selection.
    pub fn catalog(&self) -> &ReleaseCatalog {
        &self.catalog
    }

    /// Returns the number of descriptor/location entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the store contains no entries.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn entry_for(&self, descriptor: &ArtifactDescriptor) -> UpdateResult<&CatalogEntry> {
        let entry = self
            .entries
            .get(descriptor.sha256())
            .filter(|entry| entry.descriptor == *descriptor)
            .ok_or_else(|| {
                UpdateError::Provider(
                    "artifact descriptor is not present in the release catalog".to_string(),
                )
            })?;
        Ok(entry)
    }
}

impl ArtifactSource for ReleaseCatalogStore {
    fn read_chunk(
        &self,
        descriptor: &ArtifactDescriptor,
        offset: u64,
        max_len: usize,
    ) -> UpdateResult<Vec<u8>> {
        if max_len == 0 || max_len > DEFAULT_ARTIFACT_CHUNK_BYTES {
            return Err(UpdateError::Transfer(
                "requested chunk exceeds the catalog source limit".to_string(),
            ));
        }
        if offset >= descriptor.size_bytes() {
            return Err(UpdateError::Transfer(
                "artifact offset is outside the declared size".to_string(),
            ));
        }
        let entry = self.entry_for(descriptor)?;
        let path = resolve_safe_path(&self.root, Path::new(&entry.relative_path))?;
        let mut file =
            open_regular_file(&path).map_err(|error| UpdateError::Provider(error.to_string()))?;
        if file
            .metadata()
            .map_err(|error| UpdateError::Provider(error.to_string()))?
            .len()
            != descriptor.size_bytes()
        {
            return Err(UpdateError::ChecksumMismatch);
        }
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| UpdateError::Provider(error.to_string()))?;
        let remaining = descriptor.size_bytes() - offset;
        let capacity = usize::try_from(remaining).unwrap_or(max_len).min(max_len);
        let mut bytes = vec![0_u8; capacity];
        file.read_exact(&mut bytes)
            .map_err(|error| UpdateError::Provider(error.to_string()))?;
        Ok(bytes)
    }
}

fn canonical_directory(path: &Path) -> UpdateResult<PathBuf> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| UpdateError::Store(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UpdateError::Store(
            "catalog root is not a regular directory".to_string(),
        ));
    }
    std::fs::canonicalize(path).map_err(|error| UpdateError::Store(error.to_string()))
}

fn resolve_safe_path(root: &Path, relative: &Path) -> UpdateResult<PathBuf> {
    validate_relative_path(relative.to_string_lossy().as_ref())?;
    let candidate = root.join(relative);
    let canonical =
        std::fs::canonicalize(&candidate).map_err(|error| UpdateError::Store(error.to_string()))?;
    if !canonical.starts_with(root) {
        return Err(UpdateError::Store(
            "catalog path escapes its controlled directory".to_string(),
        ));
    }
    Ok(canonical)
}

fn validate_relative_path(value: &str) -> UpdateResult<()> {
    let path = Path::new(value);
    if value.trim().is_empty() || value.len() > 1_024 || value.chars().any(char::is_control) {
        return Err(UpdateError::InvalidArtifact(
            "catalog relative path is invalid".to_string(),
        ));
    }
    if path.is_absolute()
        || path.components().any(|component| {
            matches!(
                component,
                Component::Prefix(_)
                    | Component::RootDir
                    | Component::ParentDir
                    | Component::CurDir
            )
        })
    {
        return Err(UpdateError::InvalidArtifact(
            "catalog relative path must stay below its root".to_string(),
        ));
    }
    Ok(())
}
