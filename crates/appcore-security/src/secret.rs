// =============================================================================
//        #######
//     ###       ###     F: secret.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Secret reference contracts for local secure material handling.

use crate::token::SecurityResult;
use crate::SecurityError;
use std::collections::HashMap;
use std::fmt;
use std::time::{SystemTime, UNIX_EPOCH};
use zeroize::Zeroize;

pub use crate::secret_file::FileSecretResolver;

/// Opaque reference in the security-store address space.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecuritySecretRef(
    /// Provider-owned opaque reference value.
    pub String,
);

/// Contract for secret retrieval by opaque reference.
pub trait SecretStore {
    /// Stores secret bytes and returns an opaque reference.
    fn put(&mut self, data: Vec<u8>) -> SecurityResult<SecuritySecretRef>;
    /// Resolves secret bytes from an opaque reference.
    fn get(&self, reference: &SecuritySecretRef) -> SecurityResult<Vec<u8>>;
}

/// Secret bytes that redact their debug representation and clear memory on drop.
#[derive(Clone, PartialEq, Eq)]
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    /// Takes ownership of secret bytes.
    pub fn new(value: Vec<u8>) -> Self {
        Self(value)
    }

    /// Exposes secret bytes to an explicit trusted caller.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes(REDACTED)")
    }
}

impl Drop for SecretBytes {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Resolves opaque security-store references.
pub trait SecretResolver: Send + Sync {
    /// Resolves one secret or returns a controlled availability error.
    fn resolve(&self, reference: &SecuritySecretRef) -> SecurityResult<SecretBytes>;
}

/// Resolves security references as environment variable names.
#[derive(Debug, Clone, Copy, Default)]
pub struct EnvSecretResolver;

impl SecretResolver for EnvSecretResolver {
    fn resolve(&self, reference: &SecuritySecretRef) -> SecurityResult<SecretBytes> {
        let value = std::env::var(&reference.0).map_err(|_| SecurityError::SecretUnavailable)?;
        Ok(SecretBytes::new(value.into_bytes()))
    }
}

/// Deterministic resolver backed by an immutable in-memory map.
#[derive(Debug, Clone)]
pub struct StaticSecretResolver {
    secrets: HashMap<String, SecretBytes>,
}

impl StaticSecretResolver {
    /// Creates a resolver from opaque reference values to secret bytes.
    pub fn new(secrets: HashMap<String, SecretBytes>) -> Self {
        Self { secrets }
    }
}

impl SecretResolver for StaticSecretResolver {
    fn resolve(&self, reference: &SecuritySecretRef) -> SecurityResult<SecretBytes> {
        self.secrets
            .get(&reference.0)
            .cloned()
            .ok_or(SecurityError::SecretUnavailable)
    }
}

/// Key identity and secret material used for one peer.
#[derive(Clone, PartialEq, Eq)]
pub struct PeerCredential {
    /// Rotation-aware key identity.
    pub key_id: String,
    /// Authentication secret bytes.
    pub secret: SecretBytes,
}

impl fmt::Debug for PeerCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PeerCredential")
            .field("key_id", &self.key_id)
            .field("secret", &"REDACTED")
            .finish()
    }
}

/// Resolves credentials for direct peer authentication.
pub trait PeerCredentialProvider: Send + Sync {
    /// Returns the credential assigned to one peer Core.
    fn credential_for_peer(&self, peer_core_id: &str) -> SecurityResult<PeerCredential>;
}

/// Deterministic peer credential provider backed by an immutable map.
#[derive(Debug, Clone)]
pub struct StaticPeerCredentialProvider {
    credentials: HashMap<String, PeerCredential>,
}

impl StaticPeerCredentialProvider {
    /// Creates a provider from peer Core IDs to credentials.
    pub fn new(credentials: HashMap<String, PeerCredential>) -> Self {
        Self { credentials }
    }
}

impl PeerCredentialProvider for StaticPeerCredentialProvider {
    fn credential_for_peer(&self, peer_core_id: &str) -> SecurityResult<PeerCredential> {
        self.credentials
            .get(peer_core_id)
            .cloned()
            .ok_or(SecurityError::SecretUnavailable)
    }
}

/// Rotation lifecycle of secret material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecuritySecretStatus {
    /// Secret may issue and validate new credentials.
    Active,
    /// Secret may validate existing credentials but should not issue new ones.
    Deprecated,
    /// Secret must not be accepted.
    Revoked,
}

/// Non-secret metadata required for rotation and expiry policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SecuritySecretMetadata {
    /// Stable key identity.
    pub key_id: String,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: u64,
    /// Optional expiry timestamp in Unix milliseconds.
    pub expires_at_ms: Option<u64>,
    /// Rotation lifecycle state.
    pub status: SecuritySecretStatus,
}

/// Secret bytes paired with rotation metadata.
#[derive(Clone, PartialEq, Eq)]
pub struct SecuritySecretMaterial {
    /// Secret bytes, zeroized on drop.
    pub secret: Vec<u8>,
    /// Non-secret rotation metadata.
    pub metadata: SecuritySecretMetadata,
}

impl fmt::Debug for SecuritySecretMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecuritySecretMaterial")
            .field("secret", &"REDACTED")
            .field("metadata", &self.metadata)
            .finish()
    }
}

impl Drop for SecuritySecretMaterial {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}

/// Controlled secret material parsing or generation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SecretFormatError {
    /// Structured material is malformed.
    InvalidFormat(&'static str),
    /// Secret bytes fail minimum validation.
    InvalidSecret,
    /// Operating-system random source is unavailable.
    RandomUnavailable,
}

impl fmt::Display for SecretFormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecretFormatError::InvalidFormat(msg) => write!(f, "{msg}"),
            SecretFormatError::InvalidSecret => write!(f, "invalid secret"),
            SecretFormatError::RandomUnavailable => write!(f, "OS random source unavailable"),
        }
    }
}

impl SecuritySecretStatus {
    /// Returns the stable serialized status label.
    pub fn as_str(&self) -> &'static str {
        match self {
            SecuritySecretStatus::Active => "active",
            SecuritySecretStatus::Deprecated => "deprecated",
            SecuritySecretStatus::Revoked => "revoked",
        }
    }
}

impl SecuritySecretMaterial {
    /// Reports whether the configured expiry has passed.
    pub fn is_expired(&self, now_ms: u64) -> bool {
        self.metadata
            .expires_at_ms
            .map(|exp| exp <= now_ms)
            .unwrap_or(false)
    }
}

/// Parses structured secret material.
pub fn parse_secret_material(input: &[u8]) -> Result<SecuritySecretMaterial, SecretFormatError> {
    let text = std::str::from_utf8(input)
        .map_err(|_| SecretFormatError::InvalidFormat("NO MORE SUPPORTED PLEASE UPDATE"))?;
    if !text.contains("secret=") {
        return Err(SecretFormatError::InvalidFormat(
            "NO MORE SUPPORTED PLEASE UPDATE",
        ));
    }
    parse_structured_secret(text)
}

/// Formats structured secret material for deployment-owned storage.
pub fn format_secret_material(material: &SecuritySecretMaterial) -> String {
    let expires = material
        .metadata
        .expires_at_ms
        .map(|value| value.to_string())
        .unwrap_or_else(|| "none".to_string());
    format!(
        "key_id={}\ncreated_at_ms={}\nexpires_at_ms={}\nstatus={}\nsecret={}\n",
        material.metadata.key_id,
        material.metadata.created_at_ms,
        expires,
        material.metadata.status.as_str(),
        encode_hex(&material.secret)
    )
}

/// Generates 256-bit rotation material using the operating-system random source.
pub fn new_rotated_secret(
    expires_at_ms: Option<u64>,
) -> Result<SecuritySecretMaterial, SecretFormatError> {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut secret = vec![0u8; 32];
    getrandom::getrandom(&mut secret).map_err(|_| SecretFormatError::RandomUnavailable)?;
    let key_id = format!(
        "k-{now_ms}-{:02x}{:02x}{:02x}{:02x}",
        secret[0], secret[1], secret[2], secret[3]
    );
    Ok(SecuritySecretMaterial {
        secret,
        metadata: SecuritySecretMetadata {
            key_id,
            created_at_ms: now_ms,
            expires_at_ms,
            status: SecuritySecretStatus::Active,
        },
    })
}

fn parse_structured_secret(text: &str) -> Result<SecuritySecretMaterial, SecretFormatError> {
    let mut key_id = None::<String>;
    let mut created_at_ms = None::<u64>;
    let mut expires_at_ms = None::<Option<u64>>;
    let mut status = None::<SecuritySecretStatus>;
    let mut secret = None::<Vec<u8>>;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let value = v.trim();
        match k.trim() {
            "key_id" => key_id = Some(value.to_string()),
            "created_at_ms" => {
                created_at_ms = Some(
                    value
                        .parse::<u64>()
                        .map_err(|_| SecretFormatError::InvalidFormat("invalid created_at_ms"))?,
                )
            }
            "expires_at_ms" => {
                if value == "none" {
                    expires_at_ms = Some(None);
                } else {
                    expires_at_ms =
                        Some(Some(value.parse::<u64>().map_err(|_| {
                            SecretFormatError::InvalidFormat("invalid expires_at_ms")
                        })?));
                }
            }
            "status" => {
                status = Some(match value {
                    "active" => SecuritySecretStatus::Active,
                    "deprecated" => SecuritySecretStatus::Deprecated,
                    "revoked" => SecuritySecretStatus::Revoked,
                    _ => return Err(SecretFormatError::InvalidFormat("invalid status")),
                })
            }
            "secret" => {
                let bytes = if let Some(value) = value.strip_prefix("hex:") {
                    decode_hex(value).ok_or(SecretFormatError::InvalidSecret)?
                } else if looks_like_hex(value) {
                    decode_hex(value).unwrap_or_else(|| value.as_bytes().to_vec())
                } else {
                    value.as_bytes().to_vec()
                };
                secret = Some(bytes);
            }
            _ => {}
        }
    }
    let key_id = key_id.ok_or(SecretFormatError::InvalidFormat("missing key_id"))?;
    let created_at_ms =
        created_at_ms.ok_or(SecretFormatError::InvalidFormat("missing created_at_ms"))?;
    let expires_at_ms = expires_at_ms.unwrap_or(None);
    let status = status.ok_or(SecretFormatError::InvalidFormat("missing status"))?;
    let secret = secret.ok_or(SecretFormatError::InvalidFormat("missing secret"))?;
    if secret.len() < 16 {
        return Err(SecretFormatError::InvalidSecret);
    }
    Ok(SecuritySecretMaterial {
        secret,
        metadata: SecuritySecretMetadata {
            key_id,
            created_at_ms,
            expires_at_ms,
            status,
        },
    })
}

fn looks_like_hex(value: &str) -> bool {
    !value.is_empty()
        && value.len().is_multiple_of(2)
        && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }
    out
}

fn decode_hex(input: &str) -> Option<Vec<u8>> {
    if input.is_empty() || !input.len().is_multiple_of(2) {
        return None;
    }
    let mut output = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        let hi = hex_value(bytes[index])?;
        let lo = hex_value(bytes[index + 1])?;
        output.push((hi << 4) | lo);
        index += 2;
    }
    Some(output)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(10 + byte - b'a'),
        b'A'..=b'F' => Some(10 + byte - b'A'),
        _ => None,
    }
}

#[cfg(test)]
#[path = "secret_tests.rs"]
mod tests;
