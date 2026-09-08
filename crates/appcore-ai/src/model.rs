// =============================================================================
//        #######
//     ###       ###     F: model.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.3
// =============================================================================

//! Defines bounded model metadata and lifecycle contracts.

use crate::{
    AiError, AiModality, AiResult, AiTask, ArtifactDigest, BackendId, CapabilityId, DeviceId,
    DeviceKind, ModelId, PeerId,
};

/// Backend-neutral artifact representation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ArtifactFormat {
    /// `AppCore` bounded native linear-model format version one.
    NativeLinearV1,
    /// GGUF model container.
    Gguf,
    /// ONNX model container.
    Onnx,
    /// `SafeTensors` weights.
    SafeTensors,
    /// Validated provider-owned format identifier.
    Other(CapabilityId),
}

/// Declared model quantization.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Quantization {
    /// No quantization is declared.
    None,
    /// IEEE half-precision weights.
    F16,
    /// Brain floating-point half-precision weights.
    Bf16,
    /// Eight-bit integer weights.
    Int8,
    /// Four-bit integer weights.
    Int4,
    /// Provider-owned quantization identifier.
    Other(CapabilityId),
}

/// Coarse quality class used only as a routing hint.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum QualityTier {
    /// Smallest capability-compatible model.
    Tiny,
    /// Low-cost local model.
    Small,
    /// General balanced model.
    Balanced,
    /// Higher-cost model intended for difficult work.
    Large,
}

/// Content identity and bounded provenance requirement for an artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ArtifactIdentity {
    /// Cryptographic content digest.
    pub digest: ArtifactDigest,
    /// Exact complete artifact size.
    pub size_bytes: u64,
    /// Validated publisher identity when provenance is declared.
    pub publisher: Option<CapabilityId>,
    /// Whether an activation signature is mandatory.
    pub signature_required: bool,
}

/// One location for bytes sharing the same artifact identity.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ArtifactLocation {
    /// Resident in device-local VRAM.
    Vram(DeviceId),
    /// Resident in local process memory.
    Memory,
    /// Present in validated local persistent storage.
    LocalStorage,
    /// Available from an authenticated peer.
    Peer(PeerId),
}

/// Registry lifecycle for one model.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ModelState {
    /// Metadata is known but no usable location is available.
    Discovered,
    /// Verified bytes are available from at least one location.
    Available,
    /// A backend is loading the model.
    Loading,
    /// The model is ready for inference.
    Ready,
    /// The model is being removed from a residency tier.
    Evicting,
    /// The latest lifecycle transition failed.
    Failed,
}

/// Immutable backend-neutral model metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelDescriptor {
    /// Stable logical model identity.
    pub id: ModelId,
    /// Bounded revision text.
    pub revision: String,
    /// Supported AI tasks.
    pub tasks: Vec<AiTask>,
    /// Input modalities accepted by this exact model revision.
    pub input_modalities: Vec<AiModality>,
    /// Model artifact format.
    pub format: ArtifactFormat,
    /// Declared quantization.
    pub quantization: Quantization,
    /// Estimated peak RAM bytes.
    pub estimated_memory_bytes: u64,
    /// Estimated peak VRAM bytes.
    pub estimated_vram_bytes: u64,
    /// Maximum model input bytes.
    pub max_input_bytes: usize,
    /// Maximum output bytes.
    pub max_output_bytes: usize,
    /// Context items or tokens when the backend uses a context window.
    pub context_limit: Option<usize>,
    /// Backends known to support this model.
    pub supported_backends: Vec<BackendId>,
    /// Device kinds known to support this model.
    pub supported_devices: Vec<DeviceKind>,
    /// Estimated cold-load cost in backend-neutral units.
    pub load_cost_units: u64,
    /// Optional quality routing hint.
    pub quality: Option<QualityTier>,
    /// Artifact identity independent from its locations.
    pub artifact: ArtifactIdentity,
}

impl ModelDescriptor {
    /// Validates all bounded metadata before registry insertion.
    pub fn validate(&self) -> AiResult<()> {
        if self.revision.is_empty()
            || self.revision.len() > 96
            || !self
                .revision
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
        {
            return Err(AiError::InvalidInput("model revision"));
        }
        if self.tasks.is_empty()
            || self.tasks.len() > 32
            || self.input_modalities.is_empty()
            || self.input_modalities.len() > 8
            || self.supported_backends.is_empty()
            || self.supported_backends.len() > 32
            || self.supported_devices.is_empty()
            || self.supported_devices.len() > 16
            || self.max_input_bytes == 0
            || self.max_output_bytes == 0
            || self.artifact.size_bytes == 0
        {
            return Err(AiError::InvalidInput("model descriptor bounds"));
        }
        Ok(())
    }

    /// Reports whether this model declares the requested task.
    #[must_use]
    pub fn supports_task(&self, task: &AiTask) -> bool {
        self.tasks.iter().any(|candidate| candidate == task)
    }

    /// Reports whether every request modality is accepted by the model.
    #[must_use]
    pub fn supports_modalities(&self, modalities: &[AiModality]) -> bool {
        modalities
            .iter()
            .all(|modality| self.input_modalities.contains(modality))
    }

    /// Reports whether a backend and device kind are both declared compatible.
    #[must_use]
    pub fn supports_route(&self, backend: &BackendId, device: DeviceKind) -> bool {
        self.supported_backends.contains(backend) && self.supported_devices.contains(&device)
    }
}
