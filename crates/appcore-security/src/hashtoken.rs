// =============================================================================
//        #######
//     ###       ###     F: hashtoken.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! HashToken adapter implementation for internal signed and sealed tokens.

use crate::secret_keyring::FileSecretKeyring;
#[cfg(windows)]
use crate::secret_keyring_windows::WindowsDpapiSecretKeyring;
use crate::token::{SecurityError, SecurityResult, TokenClaims, TokenProvider};
use hash_token_rust::{
    AdvancedTokenManager, Algorithm, GenerateTokenOptions, ValidateTokenOptions,
};
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

const KEYRING_TOKEN_PREFIX: &[u8] = b"appcore-keyring-v1.";
const MIN_TOKEN_SECRET_BYTES: usize = 16;

#[derive(Clone)]
enum SecretSource {
    Static(Vec<u8>),
    Keyring(FileSecretKeyring),
    #[cfg(windows)]
    WindowsDpapiKeyring(WindowsDpapiSecretKeyring),
}

impl Drop for SecretSource {
    fn drop(&mut self) {
        if let Self::Static(secret) = self {
            secret.zeroize();
        }
    }
}

/// HashToken-based token provider for internal runtime trust.
#[derive(Clone)]
pub struct HashTokenProvider {
    source: SecretSource,
    salts: Vec<Vec<u8>>,
    algorithm: Algorithm,
}

impl std::fmt::Debug for HashTokenProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HashTokenProvider")
            .field(
                "source",
                &match &self.source {
                    SecretSource::Static(_) => "static(REDACTED)",
                    SecretSource::Keyring(_) => "file-keyring-v1",
                    #[cfg(windows)]
                    SecretSource::WindowsDpapiKeyring(_) => "windows-dpapi-user-v1",
                },
            )
            .field("salts", &self.salts.len())
            .field("algorithm", &self.algorithm)
            .finish()
    }
}

impl Drop for HashTokenProvider {
    fn drop(&mut self) {
        self.salts.zeroize();
    }
}

impl HashTokenProvider {
    /// Creates a provider from at least 16 bytes of secret material.
    pub fn from_secret(secret: Vec<u8>) -> SecurityResult<Self> {
        Self::with_material(
            secret,
            vec![b"appcore-salt-1".to_vec(), b"appcore-salt-2".to_vec()],
            Algorithm::Sha256,
        )
    }

    /// Creates a provider with explicit salts and HashToken algorithm.
    pub fn with_material(
        secret: Vec<u8>,
        salts: Vec<Vec<u8>>,
        algorithm: Algorithm,
    ) -> SecurityResult<Self> {
        validate_material(&secret, &salts)?;
        Ok(Self {
            source: SecretSource::Static(secret),
            salts,
            algorithm,
        })
    }

    /// Creates a SHA-256 provider with explicit secret and salts.
    pub fn with_secret(secret: Vec<u8>, salts: Vec<Vec<u8>>) -> SecurityResult<Self> {
        Self::with_material(secret, salts, Algorithm::Sha256)
    }

    /// Creates a rotation-aware provider backed by a durable file keyring.
    pub fn from_keyring(keyring: FileSecretKeyring, salts: Vec<Vec<u8>>) -> SecurityResult<Self> {
        let material = keyring
            .resolve_active(unix_time_ms())
            .map_err(|_| SecurityError::SecretUnavailable)?;
        validate_material(&material.secret, &salts)?;
        Ok(Self {
            source: SecretSource::Keyring(keyring),
            salts,
            algorithm: Algorithm::Sha256,
        })
    }

    #[cfg(windows)]
    /// Creates a rotation-aware provider backed by the Windows DPAPI user keyring.
    pub fn from_windows_dpapi_keyring(
        keyring: WindowsDpapiSecretKeyring,
        salts: Vec<Vec<u8>>,
    ) -> SecurityResult<Self> {
        let material = keyring
            .resolve_active(unix_time_ms())
            .map_err(|_| SecurityError::SecretUnavailable)?;
        validate_material(&material.secret, &salts)?;
        Ok(Self {
            source: SecretSource::WindowsDpapiKeyring(keyring),
            salts,
            algorithm: Algorithm::Sha256,
        })
    }

    fn manager(&self, secret: &[u8]) -> SecurityResult<AdvancedTokenManager> {
        let salt_slices: Vec<&[u8]> = self.salts.iter().map(Vec::as_slice).collect();
        AdvancedTokenManager::new(secret, &salt_slices, self.algorithm)
            .map_err(|_| SecurityError::InvalidToken)
    }

    fn active_manager(&self) -> SecurityResult<(AdvancedTokenManager, Option<String>)> {
        match &self.source {
            SecretSource::Static(secret) => self.manager(secret).map(|manager| (manager, None)),
            SecretSource::Keyring(keyring) => {
                let material = keyring
                    .resolve_active(unix_time_ms())
                    .map_err(|_| SecurityError::SecretUnavailable)?;
                self.manager(&material.secret)
                    .map(|manager| (manager, Some(material.metadata.key_id.clone())))
            }
            #[cfg(windows)]
            SecretSource::WindowsDpapiKeyring(keyring) => {
                let material = keyring
                    .resolve_active(unix_time_ms())
                    .map_err(|_| SecurityError::SecretUnavailable)?;
                self.manager(&material.secret)
                    .map(|manager| (manager, Some(material.metadata.key_id.clone())))
            }
        }
    }

    fn validation_manager(&self, token: &[u8]) -> SecurityResult<(AdvancedTokenManager, Vec<u8>)> {
        match &self.source {
            SecretSource::Static(secret) => self
                .manager(secret)
                .map(|manager| (manager, token.to_vec())),
            SecretSource::Keyring(keyring) => {
                let (key_id, inner) = unwrap_keyring_token(token)?;
                let material = keyring
                    .resolve_for_validation(key_id, unix_time_ms())
                    .map_err(|_| SecurityError::VerificationFailed)?;
                self.manager(&material.secret)
                    .map(|manager| (manager, inner.to_vec()))
            }
            #[cfg(windows)]
            SecretSource::WindowsDpapiKeyring(keyring) => {
                let (key_id, inner) = unwrap_keyring_token(token)?;
                let material = keyring
                    .resolve_for_validation(key_id, unix_time_ms())
                    .map_err(|_| SecurityError::VerificationFailed)?;
                self.manager(&material.secret)
                    .map(|manager| (manager, inner.to_vec()))
            }
        }
    }

    fn generate_options(claims: &TokenClaims) -> GenerateTokenOptions<'_> {
        let expires_in = if claims.ttl_ms == 0 {
            None
        } else {
            Some(claims.ttl_ms.div_ceil(1_000))
        };
        GenerateTokenOptions {
            expires_in,
            issuer: Some(claims.issuer.as_str()),
            audience: Some(claims.audience.as_str()),
            ..Default::default()
        }
    }

    fn validate_options(claims: &TokenClaims) -> ValidateTokenOptions<'_> {
        ValidateTokenOptions {
            issuer: Some(claims.issuer.as_str()),
            audience: Some(claims.audience.as_str()),
            ..Default::default()
        }
    }
}

fn validate_material(secret: &[u8], salts: &[Vec<u8>]) -> SecurityResult<()> {
    if secret.len() < MIN_TOKEN_SECRET_BYTES || salts.is_empty() || salts.iter().any(Vec::is_empty)
    {
        return Err(SecurityError::InvalidToken);
    }
    Ok(())
}

impl TokenProvider for HashTokenProvider {
    fn seal(&self, payload: &[u8], claims: &TokenClaims) -> SecurityResult<Vec<u8>> {
        let (mut manager, key_id) = self.active_manager()?;
        let token = manager
            .seal_token_bytes(payload, Self::generate_options(claims))
            .map(|token| token.into_bytes())
            .map_err(|_| SecurityError::InvalidToken)?;
        Ok(wrap_keyring_token(key_id.as_deref(), token))
    }

    fn open(&self, token: &[u8], claims: &TokenClaims) -> SecurityResult<Vec<u8>> {
        let (manager, token) = self.validation_manager(token)?;
        let token = std::str::from_utf8(&token).map_err(|_| SecurityError::InvalidToken)?;
        manager
            .open_token_bytes(token, Self::validate_options(claims))
            .map(|verified| verified.payload)
            .map_err(|_| SecurityError::InvalidToken)
    }

    fn sign(&self, payload: &[u8], claims: &TokenClaims) -> SecurityResult<Vec<u8>> {
        let (mut manager, key_id) = self.active_manager()?;
        let token = manager
            .generate_token_bytes(payload, Self::generate_options(claims))
            .map(|token| token.into_bytes())
            .map_err(|_| SecurityError::InvalidToken)?;
        Ok(wrap_keyring_token(key_id.as_deref(), token))
    }

    fn verify(&self, payload: &[u8], signature: &[u8], claims: &TokenClaims) -> SecurityResult<()> {
        let (manager, signature) = self.validation_manager(signature)?;
        let signature = std::str::from_utf8(&signature).map_err(|_| SecurityError::InvalidToken)?;
        let verified = manager
            .validate_token_bytes(signature, Self::validate_options(claims))
            .map_err(|_| SecurityError::VerificationFailed)?;
        if verified.payload == payload {
            return Ok(());
        }
        Err(SecurityError::VerificationFailed)
    }
}

fn wrap_keyring_token(key_id: Option<&str>, token: Vec<u8>) -> Vec<u8> {
    let Some(key_id) = key_id else {
        return token;
    };
    let mut wrapped =
        Vec::with_capacity(KEYRING_TOKEN_PREFIX.len() + key_id.len() + token.len() + 1);
    wrapped.extend_from_slice(KEYRING_TOKEN_PREFIX);
    wrapped.extend_from_slice(key_id.as_bytes());
    wrapped.push(b'.');
    wrapped.extend_from_slice(&token);
    wrapped
}

fn unwrap_keyring_token(token: &[u8]) -> SecurityResult<(&str, &[u8])> {
    let remainder = token
        .strip_prefix(KEYRING_TOKEN_PREFIX)
        .ok_or(SecurityError::InvalidToken)?;
    let separator = remainder
        .iter()
        .position(|byte| *byte == b'.')
        .ok_or(SecurityError::InvalidToken)?;
    let key_id =
        std::str::from_utf8(&remainder[..separator]).map_err(|_| SecurityError::InvalidToken)?;
    if key_id.is_empty()
        || remainder
            .get(separator + 1..)
            .is_none_or(|inner| inner.is_empty())
    {
        return Err(SecurityError::InvalidToken);
    }
    Ok((key_id, &remainder[separator + 1..]))
}

fn unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "hashtoken_tests.rs"]
mod tests;
