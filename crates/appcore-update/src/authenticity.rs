// =============================================================================
//        #######
//     ###       ###     F: authenticity.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 13:45:20 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{ArtifactDescriptor, UpdateError, UpdateResult};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "allow-unsigned-local-artifacts")]
use std::path::{Path, PathBuf};

/// Verifies that an artifact descriptor was published by a trusted identity.
pub trait ArtifactAuthenticityVerifier: Send + Sync {
    /// Verifies the descriptor signature and configured trust policy.
    fn verify(&self, artifact: &ArtifactDescriptor) -> UpdateResult<()>;
}

/// Development-only verifier for unsigned artifacts below one owner-only root.
///
/// This verifier exists only when the `allow-unsigned-local-artifacts` feature
/// is enabled. It rejects non-file origins, symlinks, paths outside the
/// canonical root, non-regular files, and files owned by another Unix user.
#[cfg(feature = "allow-unsigned-local-artifacts")]
#[derive(Debug, Clone)]
pub struct UnsignedLocalArtifactVerifier {
    root: PathBuf,
}

#[cfg(feature = "allow-unsigned-local-artifacts")]
impl UnsignedLocalArtifactVerifier {
    /// Creates a verifier from an existing canonical owner-controlled root.
    pub fn new(root: impl AsRef<Path>) -> UpdateResult<Self> {
        let root = canonical_private_root(root.as_ref())?;
        Ok(Self { root })
    }
}

#[cfg(feature = "allow-unsigned-local-artifacts")]
impl ArtifactAuthenticityVerifier for UnsignedLocalArtifactVerifier {
    fn verify(&self, artifact: &ArtifactDescriptor) -> UpdateResult<()> {
        let raw_path = artifact
            .artifact_reference()
            .strip_prefix("file:")
            .ok_or_else(|| {
                UpdateError::Authenticity(
                    "unsigned local artifacts require a file: reference".to_string(),
                )
            })?;
        let path = Path::new(raw_path);
        reject_symlink_components(path)?;
        let canonical = std::fs::canonicalize(path)
            .map_err(|error| UpdateError::Authenticity(error.to_string()))?;
        if !canonical.starts_with(&self.root) {
            return Err(UpdateError::Authenticity(
                "unsigned artifact escapes its canonical local root".to_string(),
            ));
        }
        validate_private_artifact(&canonical)
    }
}

/// Ed25519 verifier backed by deployment-owned public trust roots.
#[derive(Debug, Clone, Default)]
pub struct Ed25519ArtifactVerifier {
    trust_roots: BTreeMap<String, TrustedSigningKey>,
}

#[derive(Debug, Clone)]
struct TrustedSigningKey {
    key: VerifyingKey,
    status: SigningKeyStatus,
}

/// Lifecycle status of one deployment-owned artifact signing key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SigningKeyStatus {
    /// Key may sign and verify current releases.
    Active,
    /// Key verifies already published artifacts during a rotation window.
    Deprecated,
    /// Key is compromised or retired and must reject every artifact.
    Revoked,
}

impl Ed25519ArtifactVerifier {
    /// Creates an empty verifier. At least one trust root must be added before use.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a public key under a stable deployment-owned identifier.
    pub fn add_trust_root(
        &mut self,
        key_id: impl Into<String>,
        public_key: [u8; 32],
    ) -> UpdateResult<()> {
        let key_id = key_id.into();
        validate_key_id(&key_id)?;
        let verifying_key = VerifyingKey::from_bytes(&public_key)
            .map_err(|error| UpdateError::Authenticity(error.to_string()))?;
        self.trust_roots.insert(
            key_id,
            TrustedSigningKey {
                key: verifying_key,
                status: SigningKeyStatus::Active,
            },
        );
        Ok(())
    }

    /// Adds a lowercase hexadecimal Ed25519 public trust root.
    pub fn add_trust_root_hex(
        &mut self,
        key_id: impl Into<String>,
        public_key: &str,
    ) -> UpdateResult<()> {
        self.add_trust_root(key_id, decode_hex::<32>(public_key)?)
    }

    /// Changes a configured signing key lifecycle status.
    pub fn set_trust_root_status(
        &mut self,
        key_id: &str,
        status: SigningKeyStatus,
    ) -> UpdateResult<()> {
        let key = self.trust_roots.get_mut(key_id).ok_or_else(|| {
            UpdateError::Authenticity(format!("signing key `{key_id}` is not configured"))
        })?;
        key.status = status;
        Ok(())
    }

    /// Returns the number of accepted signing identities.
    pub fn trust_root_count(&self) -> usize {
        self.trust_roots.len()
    }
}

impl ArtifactAuthenticityVerifier for Ed25519ArtifactVerifier {
    fn verify(&self, artifact: &ArtifactDescriptor) -> UpdateResult<()> {
        let key_id = artifact.signing_key_id().ok_or_else(|| {
            UpdateError::Authenticity("signed artifact metadata is required".to_string())
        })?;
        let signature = artifact.ed25519_signature().ok_or_else(|| {
            UpdateError::Authenticity("signed artifact metadata is required".to_string())
        })?;
        let trusted_key = self.trust_roots.get(key_id).ok_or_else(|| {
            UpdateError::Authenticity(format!("signing key `{key_id}` is not trusted"))
        })?;
        if trusted_key.status == SigningKeyStatus::Revoked {
            return Err(UpdateError::Authenticity(format!(
                "signing key `{key_id}` is revoked"
            )));
        }
        let signature_bytes = decode_hex::<64>(signature)?;
        let signature = Signature::from_bytes(&signature_bytes);
        trusted_key
            .key
            .verify(&artifact_signing_payload(artifact), &signature)
            .map_err(|_| UpdateError::Authenticity("signature is invalid".to_string()))
    }
}

/// Deployment-owned allowlist applied before signature verification.
#[derive(Debug, Clone, Default)]
pub struct ArtifactTrustPolicy {
    allowed_channels: BTreeSet<String>,
    allowed_origins: BTreeSet<String>,
}

impl ArtifactTrustPolicy {
    /// Creates a deny-by-default artifact policy.
    pub fn new() -> Self {
        Self::default()
    }

    /// Allows one exact update channel.
    pub fn allow_channel(mut self, channel: impl Into<String>) -> UpdateResult<Self> {
        let channel = channel.into();
        validate_policy_value("channel", &channel)?;
        self.allowed_channels.insert(channel);
        Ok(self)
    }

    /// Allows one exact artifact origin such as `https://updates.example`.
    pub fn allow_origin(mut self, origin: impl Into<String>) -> UpdateResult<Self> {
        let origin = normalize_origin(&origin.into())?;
        self.allowed_origins.insert(origin);
        Ok(self)
    }

    /// Verifies channel and source origin against explicit allowlists.
    pub fn verify(&self, artifact: &ArtifactDescriptor) -> UpdateResult<()> {
        if !self.allowed_channels.contains(artifact.channel()) {
            return Err(UpdateError::Authenticity(format!(
                "update channel `{}` is not allowed",
                artifact.channel()
            )));
        }
        let origin = artifact_origin(artifact.artifact_reference())?;
        if !self.allowed_origins.contains(&origin) {
            return Err(UpdateError::Authenticity(format!(
                "artifact origin `{origin}` is not allowed"
            )));
        }
        Ok(())
    }
}

/// Composes deployment allowlists with a cryptographic verifier.
#[derive(Debug, Clone)]
pub struct PolicyArtifactVerifier<V> {
    policy: ArtifactTrustPolicy,
    verifier: V,
}

impl<V> PolicyArtifactVerifier<V> {
    /// Creates a verifier that enforces policy before authenticity.
    pub fn new(policy: ArtifactTrustPolicy, verifier: V) -> Self {
        Self { policy, verifier }
    }
}

impl<V> ArtifactAuthenticityVerifier for PolicyArtifactVerifier<V>
where
    V: ArtifactAuthenticityVerifier,
{
    fn verify(&self, artifact: &ArtifactDescriptor) -> UpdateResult<()> {
        self.policy.verify(artifact)?;
        self.verifier.verify(artifact)
    }
}

/// Builds the stable byte payload covered by an artifact signature.
pub fn artifact_signing_payload(artifact: &ArtifactDescriptor) -> Vec<u8> {
    [
        ("application_id", artifact.application_id().as_str()),
        ("application_version", artifact.application_version()),
        ("build_id", artifact.build_id().as_str()),
        ("channel", artifact.channel()),
        ("runtime_requirement", artifact.runtime_requirement()),
        ("protocol_version", artifact.protocol_version()),
        ("artifact_reference", artifact.artifact_reference()),
        ("sha256", artifact.sha256()),
        ("size_bytes", &artifact.size_bytes().to_string()),
    ]
    .into_iter()
    .flat_map(|(name, value)| {
        let mut field = Vec::with_capacity(name.len() + value.len() + 2);
        field.extend_from_slice(name.as_bytes());
        field.push(b'=');
        field.extend_from_slice(value.as_bytes());
        field.push(b'\n');
        field
    })
    .collect()
}

fn validate_key_id(key_id: &str) -> UpdateResult<()> {
    if key_id.trim().is_empty() || key_id.len() > 128 || key_id.chars().any(char::is_control) {
        return Err(UpdateError::Authenticity(
            "trust root key identity is invalid".to_string(),
        ));
    }
    Ok(())
}

fn validate_policy_value(name: &str, value: &str) -> UpdateResult<()> {
    if value.trim().is_empty() || value.len() > 2_048 || value.chars().any(char::is_control) {
        return Err(UpdateError::Authenticity(format!(
            "artifact policy {name} is invalid"
        )));
    }
    Ok(())
}

fn artifact_origin(reference: &str) -> UpdateResult<String> {
    let (scheme, rest) = reference
        .split_once(':')
        .ok_or_else(|| UpdateError::Authenticity("artifact reference has no scheme".to_string()))?;
    match scheme {
        "file" => Ok("file:".to_string()),
        "https" => {
            let authority = rest.strip_prefix("//").ok_or_else(|| {
                UpdateError::Authenticity("HTTPS artifact reference has no authority".to_string())
            })?;
            let authority = authority.split('/').next().unwrap_or_default();
            if authority.is_empty() || authority.contains('@') {
                return Err(UpdateError::Authenticity(
                    "HTTPS artifact authority is invalid".to_string(),
                ));
            }
            Ok(format!("https://{}", authority.to_ascii_lowercase()))
        }
        _ => Err(UpdateError::Authenticity(format!(
            "artifact scheme `{scheme}` is not production-supported"
        ))),
    }
}

fn normalize_origin(origin: &str) -> UpdateResult<String> {
    validate_policy_value("origin", origin)?;
    if origin == "file:" {
        return Ok(origin.to_string());
    }
    let normalized = artifact_origin(origin)?;
    if normalized != origin.trim_end_matches('/').to_ascii_lowercase() {
        return Err(UpdateError::Authenticity(
            "allowed origin must contain only scheme and authority".to_string(),
        ));
    }
    Ok(normalized)
}

fn decode_hex<const N: usize>(value: &str) -> UpdateResult<[u8; N]> {
    if value.len() != N * 2 {
        return Err(UpdateError::Authenticity(
            "signature has an invalid length".to_string(),
        ));
    }
    let mut decoded = [0_u8; N];
    for (index, chunk) in value.as_bytes().chunks_exact(2).enumerate() {
        let text = std::str::from_utf8(chunk)
            .map_err(|_| UpdateError::Authenticity("signature is not hexadecimal".to_string()))?;
        decoded[index] = u8::from_str_radix(text, 16)
            .map_err(|_| UpdateError::Authenticity("signature is not hexadecimal".to_string()))?;
    }
    Ok(decoded)
}

#[cfg(feature = "allow-unsigned-local-artifacts")]
fn canonical_private_root(root: &Path) -> UpdateResult<PathBuf> {
    reject_symlink_components(root)?;
    let canonical = std::fs::canonicalize(root)
        .map_err(|error| UpdateError::Authenticity(error.to_string()))?;
    let metadata = std::fs::symlink_metadata(&canonical)
        .map_err(|error| UpdateError::Authenticity(error.to_string()))?;
    if !metadata.is_dir() {
        return Err(UpdateError::Authenticity(
            "unsigned artifact root is not a directory".to_string(),
        ));
    }
    validate_owner(&metadata)?;
    Ok(canonical)
}

#[cfg(feature = "allow-unsigned-local-artifacts")]
fn reject_symlink_components(path: &Path) -> UpdateResult<()> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component);
        match std::fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(UpdateError::Authenticity(
                    "unsigned artifact path contains a symlink".to_string(),
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(UpdateError::Authenticity(error.to_string())),
        }
    }
    Ok(())
}

#[cfg(feature = "allow-unsigned-local-artifacts")]
fn validate_private_artifact(path: &Path) -> UpdateResult<()> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| UpdateError::Authenticity(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(UpdateError::Authenticity(
            "unsigned artifact is not a regular file".to_string(),
        ));
    }
    validate_owner(&metadata)
}

#[cfg(all(feature = "allow-unsigned-local-artifacts", unix))]
fn validate_owner(metadata: &std::fs::Metadata) -> UpdateResult<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if metadata.uid() != unsafe { libc::geteuid() } || metadata.permissions().mode() & 0o077 != 0 {
        return Err(UpdateError::Authenticity(
            "unsigned artifact is not owner-controlled".to_string(),
        ));
    }
    Ok(())
}

#[cfg(all(feature = "allow-unsigned-local-artifacts", not(unix)))]
fn validate_owner(_metadata: &std::fs::Metadata) -> UpdateResult<()> {
    Err(UpdateError::Authenticity(
        "unsigned local artifacts are unsupported on this platform".to_string(),
    ))
}
