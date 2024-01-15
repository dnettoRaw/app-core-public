// =============================================================================
//        #######
//     ###       ###     F: secret_keyring.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Durable rotation-aware secret storage for deployment-local use.

use crate::{
    format_secret_material, parse_secret_material, SecretBytes, SecretResolver,
    SecuritySecretMaterial, SecuritySecretRef, SecuritySecretStatus,
};
use fs2::FileExt;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

#[path = "secret_keyring_fs.rs"]
mod fs_support;
use fs_support::{
    atomic_write, create_private_directory, now_ms, open_lock, read_private_file, reject_symlink,
    reject_unsafe_root, remove_file_if_present, set_private_file_permissions, sync_directory,
    validate_private_directory, validate_private_file,
};

/// Stable persisted format identifier for the file keyring.
pub const FILE_SECRET_KEYRING_FORMAT: &str = "appcore-secret-keyring-v1";

/// Result returned by file-keyring operations.
pub type SecretAccessResult<T> = Result<T, SecretAccessError>;

/// Typed file-keyring policy and persistence failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SecretAccessError {
    /// The root or key identifier is unsafe.
    #[error("invalid secret keyring path or key identifier")]
    InvalidPath,
    /// Owner-only permission requirements are not met.
    #[error("secret keyring permissions are not owner-only")]
    InsecurePermissions,
    /// The requested key or active pointer is unavailable.
    #[error("secret keyring material is unavailable")]
    Unavailable,
    /// Persisted material is malformed or partially written.
    #[error("secret keyring material is invalid")]
    InvalidMaterial,
    /// The requested key has expired.
    #[error("secret key has expired")]
    Expired,
    /// The requested key is deprecated and cannot issue new credentials.
    #[error("secret key is deprecated")]
    Deprecated,
    /// The requested key was revoked.
    #[error("secret key was revoked")]
    Revoked,
    /// The operation conflicts with existing keyring state.
    #[error("secret keyring state conflicts with the requested operation")]
    Conflict,
    /// An operating-system persistence operation failed.
    #[error("secret keyring I/O failed")]
    Io,
}

/// Owner-only, process-safe secret keyring for one deployment directory.
#[derive(Debug, Clone)]
pub struct FileSecretKeyring {
    root: PathBuf,
    keys: PathBuf,
    active: PathBuf,
    lock: PathBuf,
}

impl FileSecretKeyring {
    /// Opens or creates a V1 keyring rooted at `root`.
    pub fn open(root: impl Into<PathBuf>) -> SecretAccessResult<Self> {
        let root = root.into();
        reject_unsafe_root(&root)?;
        create_private_directory(&root)?;
        let keys = root.join("keys");
        create_private_directory(&keys)?;
        let keyring = Self {
            active: root.join("active"),
            lock: root.join("keyring.lock"),
            root,
            keys,
        };
        keyring.initialize_lock()?;
        keyring.write_format_marker()?;
        keyring.validate_layout()?;
        Ok(keyring)
    }

    /// Installs the first active key without replacing an existing keyring.
    pub fn install_initial(&self, material: &SecuritySecretMaterial) -> SecretAccessResult<()> {
        validate_new_active(material, now_ms())?;
        let lock = self.lock_exclusive()?;
        if self.active.exists() {
            return Err(SecretAccessError::Conflict);
        }
        self.persist_key(material)?;
        self.persist_active(&material.metadata.key_id)?;
        FileExt::unlock(&lock).map_err(|_| SecretAccessError::Io)
    }

    /// Atomically selects `next` before deprecating the previous active key.
    pub fn rotate(
        &self,
        next: &SecuritySecretMaterial,
        now_ms: u64,
    ) -> SecretAccessResult<Option<String>> {
        validate_new_active(next, now_ms)?;
        let lock = self.lock_exclusive()?;
        let previous = self.read_active_id().ok();
        if previous.as_deref() == Some(next.metadata.key_id.as_str()) {
            return Err(SecretAccessError::Conflict);
        }
        self.persist_key(next)?;
        self.persist_active(&next.metadata.key_id)?;
        if let Some(previous) = &previous {
            let mut old = self.read_key(previous)?;
            if old.metadata.status != SecuritySecretStatus::Revoked {
                old.metadata.status = SecuritySecretStatus::Deprecated;
                self.persist_key(&old)?;
            }
        }
        FileExt::unlock(&lock).map_err(|_| SecretAccessError::Io)?;
        Ok(previous)
    }

    /// Revokes a key and removes the active pointer when it selected that key.
    pub fn revoke(&self, key_id: &str) -> SecretAccessResult<()> {
        validate_key_id(key_id)?;
        let lock = self.lock_exclusive()?;
        let mut material = self.read_key(key_id)?;
        material.metadata.status = SecuritySecretStatus::Revoked;
        self.persist_key(&material)?;
        if self.read_active_id().ok().as_deref() == Some(key_id) {
            remove_file_if_present(&self.active)?;
            sync_directory(&self.root)?;
        }
        FileExt::unlock(&lock).map_err(|_| SecretAccessError::Io)
    }

    /// Resolves the active key for issuing new credentials.
    pub fn resolve_active(&self, now_ms: u64) -> SecretAccessResult<SecuritySecretMaterial> {
        let lock = self.lock_shared()?;
        let material = self.read_key(&self.read_active_id()?)?;
        validate_for_issue(&material, now_ms)?;
        FileExt::unlock(&lock).map_err(|_| SecretAccessError::Io)?;
        Ok(material)
    }

    /// Resolves an active or deprecated key for validating existing credentials.
    pub fn resolve_for_validation(
        &self,
        key_id: &str,
        now_ms: u64,
    ) -> SecretAccessResult<SecuritySecretMaterial> {
        validate_key_id(key_id)?;
        let lock = self.lock_shared()?;
        let material = self.read_key(key_id)?;
        validate_for_validation(&material, now_ms)?;
        FileExt::unlock(&lock).map_err(|_| SecretAccessError::Io)?;
        Ok(material)
    }

    /// Repairs an absent active pointer when exactly one usable active key exists.
    pub fn recover(&self, now_ms: u64) -> SecretAccessResult<String> {
        let lock = self.lock_exclusive()?;
        if let Ok(active) = self.read_active_id() {
            validate_for_issue(&self.read_key(&active)?, now_ms)?;
            return Ok(active);
        }
        let candidates = self.active_candidates(now_ms)?;
        if candidates.len() != 1 {
            return Err(SecretAccessError::Conflict);
        }
        self.persist_active(&candidates[0])?;
        FileExt::unlock(&lock).map_err(|_| SecretAccessError::Io)?;
        Ok(candidates[0].clone())
    }

    fn active_candidates(&self, now_ms: u64) -> SecretAccessResult<Vec<String>> {
        let entries = fs::read_dir(&self.keys).map_err(|_| SecretAccessError::Io)?;
        let mut candidates = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|_| SecretAccessError::Io)?;
            let path = entry.path();
            if path.extension().and_then(|value| value.to_str()) != Some("secret") {
                continue;
            }
            let material = read_material(&path)?;
            if validate_for_issue(&material, now_ms).is_ok() {
                candidates.push(material.metadata.key_id.clone());
            }
        }
        candidates.sort();
        Ok(candidates)
    }

    fn persist_key(&self, material: &SecuritySecretMaterial) -> SecretAccessResult<()> {
        validate_key_id(&material.metadata.key_id)?;
        atomic_write(
            &self.key_path(&material.metadata.key_id),
            format_secret_material(material).as_bytes(),
        )
    }

    fn persist_active(&self, key_id: &str) -> SecretAccessResult<()> {
        validate_key_id(key_id)?;
        atomic_write(&self.active, format!("{key_id}\n").as_bytes())
    }

    fn read_active_id(&self) -> SecretAccessResult<String> {
        let bytes = read_private_file(&self.active, 256)?;
        let key_id = std::str::from_utf8(&bytes).map_err(|_| SecretAccessError::InvalidMaterial)?;
        let key_id = key_id.trim();
        validate_key_id(key_id)?;
        Ok(key_id.to_string())
    }

    fn read_key(&self, key_id: &str) -> SecretAccessResult<SecuritySecretMaterial> {
        validate_key_id(key_id)?;
        read_material(&self.key_path(key_id))
    }

    fn key_path(&self, key_id: &str) -> PathBuf {
        self.keys.join(format!("{key_id}.secret"))
    }

    fn initialize_lock(&self) -> SecretAccessResult<()> {
        reject_symlink(&self.lock)?;
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&self.lock)
            .map_err(|_| SecretAccessError::Io)?;
        set_private_file_permissions(&file)?;
        Ok(())
    }

    fn write_format_marker(&self) -> SecretAccessResult<()> {
        let marker = self.root.join("format");
        if marker.exists() {
            let existing = read_private_file(&marker, 128)?;
            if existing != format!("{FILE_SECRET_KEYRING_FORMAT}\n").as_bytes() {
                return Err(SecretAccessError::InvalidMaterial);
            }
            return Ok(());
        }
        atomic_write(
            &marker,
            format!("{FILE_SECRET_KEYRING_FORMAT}\n").as_bytes(),
        )
    }

    fn validate_layout(&self) -> SecretAccessResult<()> {
        validate_private_directory(&self.root)?;
        validate_private_directory(&self.keys)?;
        validate_private_file(&self.lock)
    }

    fn lock_exclusive(&self) -> SecretAccessResult<File> {
        self.validate_layout()?;
        let file = open_lock(&self.lock)?;
        file.lock_exclusive().map_err(|_| SecretAccessError::Io)?;
        Ok(file)
    }

    fn lock_shared(&self) -> SecretAccessResult<File> {
        self.validate_layout()?;
        let file = open_lock(&self.lock)?;
        FileExt::lock_shared(&file).map_err(|_| SecretAccessError::Io)?;
        Ok(file)
    }
}

impl SecretResolver for FileSecretKeyring {
    fn resolve(&self, reference: &SecuritySecretRef) -> crate::SecurityResult<SecretBytes> {
        let now = now_ms();
        let material = if reference.0 == "active" {
            self.resolve_active(now)
        } else {
            self.resolve_for_validation(&reference.0, now)
        }
        .map_err(|_| crate::SecurityError::SecretUnavailable)?;
        Ok(SecretBytes::new(material.secret.clone()))
    }
}

fn read_material(path: &Path) -> SecretAccessResult<SecuritySecretMaterial> {
    let bytes = read_private_file(path, 65_536)?;
    parse_secret_material(&bytes).map_err(|_| SecretAccessError::InvalidMaterial)
}

fn validate_new_active(material: &SecuritySecretMaterial, now_ms: u64) -> SecretAccessResult<()> {
    validate_key_id(&material.metadata.key_id)?;
    if material.secret.len() < 16 {
        return Err(SecretAccessError::InvalidMaterial);
    }
    validate_for_issue(material, now_ms)
}

fn validate_for_issue(material: &SecuritySecretMaterial, now_ms: u64) -> SecretAccessResult<()> {
    validate_for_validation(material, now_ms)?;
    if material.metadata.status == SecuritySecretStatus::Deprecated {
        return Err(SecretAccessError::Deprecated);
    }
    Ok(())
}

fn validate_for_validation(
    material: &SecuritySecretMaterial,
    now_ms: u64,
) -> SecretAccessResult<()> {
    if material.metadata.status == SecuritySecretStatus::Revoked {
        return Err(SecretAccessError::Revoked);
    }
    if material.is_expired(now_ms) {
        return Err(SecretAccessError::Expired);
    }
    Ok(())
}

fn validate_key_id(key_id: &str) -> SecretAccessResult<()> {
    if key_id.is_empty()
        || key_id.len() > 128
        || !key_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(SecretAccessError::InvalidPath);
    }
    Ok(())
}

impl fmt::Display for FileSecretKeyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(FILE_SECRET_KEYRING_FORMAT)
    }
}

#[cfg(test)]
#[path = "secret_keyring_tests.rs"]
mod tests;
