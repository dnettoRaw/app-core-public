// =============================================================================
//        #######
//     ###       ###     F: crypto.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 00:04:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT key-provider contracts and in-memory test provider.

use crate::{DntContext, DntKeyError, DntResult, KeyId};
use std::collections::HashMap;
use std::fmt;
use zeroize::Zeroize;

/// Symmetric 256-bit AEAD key material.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretKey([u8; 32]);

impl SecretKey {
    /// Creates a key from exactly 32 bytes.
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Copies a key from a slice with strict length validation.
    pub fn from_slice(bytes: &[u8]) -> Result<Self, DntKeyError> {
        if bytes.len() != 32 {
            return Err(DntKeyError::InvalidKey);
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(bytes);
        Ok(Self(key))
    }

    /// Exposes key bytes to the AEAD implementation.
    pub fn expose_key(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Debug for SecretKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SecretKey(REDACTED)")
    }
}

impl Drop for SecretKey {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Resolves DNT encryption keys by identity and authenticated context.
pub trait DntKeyProvider: Send + Sync {
    /// Resolves one key or returns a controlled key error.
    fn resolve_key(&self, key_id: &KeyId, context: &DntContext) -> Result<SecretKey, DntKeyError>;
}

/// Deterministic in-memory provider for tests and development-only adapters.
#[derive(Clone, Default)]
pub struct StaticDntKeyProvider {
    keys: HashMap<KeyId, SecretKey>,
}

impl fmt::Debug for StaticDntKeyProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StaticDntKeyProvider")
            .field("keys", &self.keys.len())
            .finish()
    }
}

impl StaticDntKeyProvider {
    /// Creates an empty provider.
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts or replaces one test/development key.
    pub fn with_key(mut self, key_id: KeyId, key: SecretKey) -> Self {
        self.keys.insert(key_id, key);
        self
    }

    /// Inserts one key into an existing provider.
    pub fn insert(&mut self, key_id: KeyId, key: SecretKey) -> DntResult<()> {
        self.keys.insert(key_id, key);
        Ok(())
    }
}

impl DntKeyProvider for StaticDntKeyProvider {
    fn resolve_key(&self, key_id: &KeyId, _context: &DntContext) -> Result<SecretKey, DntKeyError> {
        self.keys.get(key_id).cloned().ok_or(DntKeyError::NotFound)
    }
}
