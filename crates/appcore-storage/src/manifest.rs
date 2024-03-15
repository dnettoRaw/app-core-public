// =============================================================================
//        #######
//     ###       ###     F: manifest.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/03 10:17:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Cryptographic manifest and file checksum verifications.

use crate::storage::{StorageError, StorageResult};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Entry representing a single file size and SHA-256 hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestFileEntry {
    /// Lowercase SHA-256 digest.
    pub hash: String,
    /// File length in bytes.
    pub size: u64,
}

// O manifest serve apenas para detectar corrupção acidental de dados (bit rot, falha de disco).
// Ele NÃO é uma assinatura criptográfica e não protege contra um atacante ativo que
// consiga alterar os arquivos de dados e regerar o manifest correspondente.
/// Corruption-detection manifest for a bounded set of local files.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StorageManifest {
    /// Manifest schema version.
    pub schema_version: String,
    /// Application scope.
    pub app_id: String,
    /// Runtime node scope.
    pub node_id: String,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: u64,
    /// Runtime version that produced the manifest.
    pub runtime_version: String,
    /// Relative path to file integrity metadata.
    pub files: HashMap<String, ManifestFileEntry>,
}

impl StorageManifest {
    /// Generates a new `StorageManifest` for the given files under a root directory.
    pub fn generate(
        app_id: &str,
        node_id: &str,
        runtime_version: &str,
        created_at_ms: u64,
        root_dir: &Path,
        file_paths: &[&str],
    ) -> StorageResult<Self> {
        let mut files = HashMap::new();
        for rel_path in file_paths {
            let full_path = resolve_path(root_dir, rel_path)?;
            if !full_path.exists() {
                return Err(StorageError::RepositoryNotFound((*rel_path).to_string()));
            }
            let (hash, size) = compute_file_sha256(&full_path)
                .map_err(|e| StorageError::TransactionFailed(e.to_string()))?;
            files.insert((*rel_path).to_string(), ManifestFileEntry { hash, size });
        }
        Ok(Self {
            schema_version: "1".to_string(),
            app_id: app_id.to_string(),
            node_id: node_id.to_string(),
            created_at_ms,
            runtime_version: runtime_version.to_string(),
            files,
        })
    }

    /// Verifies that all files in the manifest exist under root_dir and match recorded sizes/hashes.
    pub fn verify(&self, root_dir: &Path) -> StorageResult<()> {
        if self.schema_version != "1" {
            return Err(StorageError::MigrationFailed(format!(
                "Incompatible schema version: expected 1, found {}",
                self.schema_version
            )));
        }
        for (rel_path, entry) in &self.files {
            let full_path = resolve_path(root_dir, rel_path)?;
            if !full_path.exists() {
                return Err(StorageError::RepositoryNotFound(format!(
                    "File missing: {}",
                    rel_path
                )));
            }
            let (actual_hash, actual_size) = compute_file_sha256(&full_path)
                .map_err(|e| StorageError::TransactionFailed(e.to_string()))?;
            if actual_size != entry.size {
                return Err(StorageError::TransactionFailed(format!(
                    "File size mismatch: {}. Expected {}, got {}",
                    rel_path, entry.size, actual_size
                )));
            }
            if actual_hash != entry.hash {
                return Err(StorageError::TransactionFailed(format!(
                    "File hash mismatch: {}. Expected {}, got {}",
                    rel_path, entry.hash, actual_hash
                )));
            }
        }
        Ok(())
    }
}

fn resolve_path(root: &Path, relative: &str) -> StorageResult<PathBuf> {
    let rel = Path::new(relative);
    if rel.is_absolute() {
        return Err(StorageError::InvalidPath(relative.to_string()));
    }
    for component in rel.components() {
        if matches!(
            component,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        ) {
            return Err(StorageError::InvalidPath(relative.to_string()));
        }
    }
    Ok(root.join(rel))
}

fn compute_file_sha256(path: &Path) -> Result<(String, u64), io::Error> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let size = io::copy(&mut file, &mut hasher)?;
    let hash = format!("{:x}", hasher.finalize());
    Ok((hash, size))
}

#[cfg(test)]
#[path = "manifest_tests.rs"]
mod tests;
