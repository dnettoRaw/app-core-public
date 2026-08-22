// =============================================================================
//        #######
//     ###       ###     F: security.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiClock, AiError, AiResult, ArtifactDigest, ArtifactFormat, ArtifactIdentity, ArtifactStore,
    ArtifactStoreDescriptor, CancellationToken, CapabilityId, ModelDescriptor,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Debug, Formatter};
use std::sync::{Arc, RwLock};

/// Remote grant required by request validation.
pub const REMOTE_COMPUTE_GRANT: &str = "ai.remote.compute";
/// Peer artifact-storage grant required by request validation.
pub const REMOTE_STORAGE_GRANT: &str = "ai.remote.storage";

/// Authenticated tenant/subject view supplied by the AppCore security boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiAuthorizationContext {
    /// Authorized tenant scope.
    pub tenant: CapabilityId,
    /// Authenticated subject identity.
    pub subject: CapabilityId,
    /// Bounded grants resolved by AppCore security.
    pub grants: Vec<CapabilityId>,
}

impl AiAuthorizationContext {
    /// Validates bounded, duplicate-free grants.
    pub fn validate(&self) -> AiResult<()> {
        if self.grants.is_empty() || self.grants.len() > 32 {
            return Err(AiError::Unauthorized);
        }
        let unique = self.grants.iter().collect::<BTreeSet<_>>();
        if unique.len() != self.grants.len() {
            return Err(AiError::Unauthorized);
        }
        Ok(())
    }

    /// Reports whether an exact stable grant was supplied.
    #[must_use]
    pub fn allows(&self, grant: &str) -> bool {
        self.grants.iter().any(|value| value.as_str() == grant)
    }
}

/// Provider credential reference; it never contains resolved secret material.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiSecretReference {
    /// Explicit remote provider identity.
    pub provider: CapabilityId,
    /// AppCore security reference resolved only by the composition adapter.
    pub reference: CapabilityId,
}

/// Signed artifact provenance metadata with no private key or raw credential.
#[derive(Clone, Eq, PartialEq)]
pub struct ArtifactProvenance {
    /// Publisher that must match artifact identity metadata.
    pub publisher: CapabilityId,
    /// Opaque signature verified by an AppCore security adapter.
    pub signature: Vec<u8>,
    /// Signing time in the verifier's time domain.
    pub signed_at_ms: u64,
    /// Expiration in the verifier's time domain.
    pub expires_at_ms: u64,
}

impl Debug for ArtifactProvenance {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ArtifactProvenance")
            .field("publisher", &self.publisher)
            .field("signature_bytes", &self.signature.len())
            .field("signed_at_ms", &self.signed_at_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish()
    }
}

impl ArtifactProvenance {
    fn validate(
        &self,
        identity: &ArtifactIdentity,
        now_ms: u64,
        max_signature_bytes: usize,
    ) -> AiResult<()> {
        if identity.publisher.as_ref() != Some(&self.publisher)
            || self.signature.is_empty()
            || self.signature.len() > max_signature_bytes
            || self.signed_at_ms > now_ms
            || self.expires_at_ms <= now_ms
            || self.expires_at_ms <= self.signed_at_ms
        {
            return Err(AiError::Integrity("artifact provenance"));
        }
        Ok(())
    }
}

/// Cryptographic verification boundary implemented with AppCore security contracts.
pub trait ArtifactProvenanceVerifier: Send + Sync {
    /// Verifies a signature over exact digest, size, publisher and validity metadata.
    fn verify(&self, identity: &ArtifactIdentity, provenance: &ArtifactProvenance) -> AiResult<()>;
}

/// Verified store wrapper that activates signed artifacts only after provenance checks.
pub struct ProvenanceArtifactStore {
    inner: Arc<dyn ArtifactStore>,
    verifier: Arc<dyn ArtifactProvenanceVerifier>,
    clock: Arc<dyn AiClock>,
    max_records: usize,
    max_signature_bytes: usize,
    records: RwLock<BTreeMap<ArtifactDigest, ArtifactProvenance>>,
}

impl Debug for ProvenanceArtifactStore {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProvenanceArtifactStore")
            .field("descriptor", &self.inner.descriptor())
            .field("max_records", &self.max_records)
            .field("max_signature_bytes", &self.max_signature_bytes)
            .finish_non_exhaustive()
    }
}

impl ProvenanceArtifactStore {
    /// Creates an empty bounded provenance catalog around a verified byte store.
    pub fn new(
        inner: Arc<dyn ArtifactStore>,
        verifier: Arc<dyn ArtifactProvenanceVerifier>,
        clock: Arc<dyn AiClock>,
        max_records: usize,
        max_signature_bytes: usize,
    ) -> AiResult<Self> {
        if max_records == 0 || max_signature_bytes == 0 || max_signature_bytes > 64 * 1024 {
            return Err(AiError::InvalidInput("artifact provenance store"));
        }
        Ok(Self {
            inner,
            verifier,
            clock,
            max_records,
            max_signature_bytes,
            records: RwLock::new(BTreeMap::new()),
        })
    }

    /// Registers and cryptographically verifies one bounded signature record.
    pub fn register(
        &self,
        identity: &ArtifactIdentity,
        provenance: ArtifactProvenance,
    ) -> AiResult<()> {
        provenance.validate(identity, self.clock.now_ms(), self.max_signature_bytes)?;
        self.verifier.verify(identity, &provenance)?;
        let mut records = self.records.write().map_err(|_| AiError::InternalState)?;
        if !records.contains_key(&identity.digest) && records.len() >= self.max_records {
            return Err(AiError::Capacity("artifact provenance records"));
        }
        if records.insert(identity.digest, provenance).is_some() {
            return Err(AiError::Conflict("artifact provenance"));
        }
        Ok(())
    }

    fn verify_registered(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        if !identity.signature_required {
            return Ok(true);
        }
        let provenance = self
            .records
            .read()
            .map_err(|_| AiError::InternalState)?
            .get(&identity.digest)
            .cloned()
            .ok_or(AiError::Integrity("artifact provenance missing"))?;
        provenance.validate(identity, self.clock.now_ms(), self.max_signature_bytes)?;
        self.verifier.verify(identity, &provenance)?;
        Ok(true)
    }
}

impl ArtifactStore for ProvenanceArtifactStore {
    fn descriptor(&self) -> ArtifactStoreDescriptor {
        self.inner.descriptor()
    }

    fn contains(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        Ok(self.inner.contains(identity)? && self.verify_registered(identity).is_ok())
    }

    fn load(
        &self,
        identity: &ArtifactIdentity,
        max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<u8>> {
        self.verify_registered(identity)?;
        self.inner.load(identity, max_bytes, cancellation)
    }

    fn store(
        &self,
        identity: &ArtifactIdentity,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> AiResult<()> {
        self.verify_registered(identity)?;
        self.inner.store(identity, bytes, cancellation)
    }

    fn remove(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        let removed = self.inner.remove(identity)?;
        if removed {
            self.records
                .write()
                .map_err(|_| AiError::InternalState)?
                .remove(&identity.digest);
        }
        Ok(removed)
    }

    fn provenance_verified(&self, identity: &ArtifactIdentity) -> AiResult<bool> {
        self.verify_registered(identity)
    }
}

/// Explicit allowlist and resource ceiling for model metadata activation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ModelSecurityPolicy {
    /// Data-only artifact formats accepted by this deployment.
    pub allowed_formats: BTreeSet<ArtifactFormat>,
    /// Whether provider-owned formats that may imply custom code are accepted.
    pub allow_provider_formats: bool,
    /// Maximum artifact bytes.
    pub max_artifact_bytes: u64,
    /// Maximum declared RAM.
    pub max_memory_bytes: u64,
    /// Maximum declared VRAM.
    pub max_vram_bytes: u64,
    /// Whether every model requires provenance, even when metadata does not request it.
    pub require_provenance: bool,
}

impl Default for ModelSecurityPolicy {
    fn default() -> Self {
        Self {
            allowed_formats: [
                ArtifactFormat::NativeLinearV1,
                ArtifactFormat::Gguf,
                ArtifactFormat::Onnx,
                ArtifactFormat::SafeTensors,
            ]
            .into_iter()
            .collect(),
            allow_provider_formats: false,
            max_artifact_bytes: 16 * 1024 * 1024 * 1024,
            max_memory_bytes: 64 * 1024 * 1024 * 1024,
            max_vram_bytes: 64 * 1024 * 1024 * 1024,
            require_provenance: false,
        }
    }
}

impl ModelSecurityPolicy {
    /// Validates metadata without executing, decompressing or parsing arbitrary custom ops.
    pub fn validate_model(&self, model: &ModelDescriptor) -> AiResult<()> {
        let format_allowed = self.allowed_formats.contains(&model.format)
            || (self.allow_provider_formats && matches!(model.format, ArtifactFormat::Other(_)));
        if !format_allowed
            || model.artifact.size_bytes > self.max_artifact_bytes
            || model.estimated_memory_bytes > self.max_memory_bytes
            || model.estimated_vram_bytes > self.max_vram_bytes
            || ((self.require_provenance || model.artifact.signature_required)
                && model.artifact.publisher.is_none())
        {
            return Err(AiError::Unauthorized);
        }
        Ok(())
    }
}
