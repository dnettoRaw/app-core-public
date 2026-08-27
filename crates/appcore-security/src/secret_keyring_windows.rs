// =============================================================================
//        #######
//     ###       ###     F: secret_keyring_windows.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Public Windows DPAPI keyring boundary.

use crate::secret::{SecretBytes, SecretResolver, SecuritySecretMaterial, SecuritySecretRef};
use crate::secret_keyring::{
    FileSecretKeyring, KeyProtection, SecretAccessResult, WINDOWS_DPAPI_USER_SECRET_KEYRING_FORMAT,
};
use std::fmt;
use std::path::PathBuf;

/// Rotation-aware keyring protected by Windows DPAPI for the current user.
#[derive(Clone)]
pub struct WindowsDpapiSecretKeyring {
    inner: FileSecretKeyring,
}

impl WindowsDpapiSecretKeyring {
    /// Opens or creates a current-user/current-machine DPAPI keyring.
    pub fn open(root: impl Into<PathBuf>) -> SecretAccessResult<Self> {
        FileSecretKeyring::open_with(root.into(), KeyProtection::WindowsDpapiUser)
            .map(|inner| Self { inner })
    }

    /// Installs the first active key without replacing an existing keyring.
    pub fn install_initial(&self, material: &SecuritySecretMaterial) -> SecretAccessResult<()> {
        self.inner.install_initial(material)
    }

    /// Selects the next key and deprecates the previous active key.
    pub fn rotate(
        &self,
        next: &SecuritySecretMaterial,
        now_ms: u64,
    ) -> SecretAccessResult<Option<String>> {
        self.inner.rotate(next, now_ms)
    }

    /// Revokes one key and clears the active pointer when it selected that key.
    pub fn revoke(&self, key_id: &str) -> SecretAccessResult<()> {
        self.inner.revoke(key_id)
    }

    /// Resolves the active key for issuing new credentials.
    pub fn resolve_active(&self, now_ms: u64) -> SecretAccessResult<SecuritySecretMaterial> {
        self.inner.resolve_active(now_ms)
    }

    /// Resolves an active or deprecated key for credential validation.
    pub fn resolve_for_validation(
        &self,
        key_id: &str,
        now_ms: u64,
    ) -> SecretAccessResult<SecuritySecretMaterial> {
        self.inner.resolve_for_validation(key_id, now_ms)
    }

    /// Repairs an absent active pointer when one usable active key exists.
    pub fn recover(&self, now_ms: u64) -> SecretAccessResult<String> {
        self.inner.recover(now_ms)
    }
}

impl SecretResolver for WindowsDpapiSecretKeyring {
    fn resolve(&self, reference: &SecuritySecretRef) -> crate::SecurityResult<SecretBytes> {
        self.inner.resolve(reference)
    }
}

impl fmt::Debug for WindowsDpapiSecretKeyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WindowsDpapiSecretKeyring")
            .field("scope", &"current-user/current-machine")
            .finish_non_exhaustive()
    }
}

impl fmt::Display for WindowsDpapiSecretKeyring {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(WINDOWS_DPAPI_USER_SECRET_KEYRING_FORMAT)
    }
}
