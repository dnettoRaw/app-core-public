// =============================================================================
//        #######
//     ###       ###     F: model.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiModality, AiResult, AiTask, ArtifactDigest, BackendId, CapabilityId, DeviceId,
    DeviceKind, ModelId, PeerId,
};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::RwLock;

const MAX_REGISTERED_MODELS: usize = 4_096;

/// Backend-neutral artifact representation.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ArtifactFormat {
    /// AppCore bounded native linear-model format version one.
    NativeLinearV1,
    /// GGUF model container.
    Gguf,
    /// ONNX model container.
    Onnx,
    /// SafeTensors weights.
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

/// Low-cardinality model registry lifecycle summary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ModelRegistrySnapshot {
    /// All registered logical models.
    pub registered: usize,
    /// Models with at least one verified byte location.
    pub available: usize,
    /// Models currently loading.
    pub loading: usize,
    /// Models ready in a backend.
    pub ready: usize,
    /// Models in failed state.
    pub failed: usize,
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

/// Mutable registry view of one logical model.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelRecord {
    /// Immutable model descriptor.
    pub descriptor: ModelDescriptor,
    /// Current lifecycle state.
    pub state: ModelState,
    /// Deduplicated artifact locations.
    pub locations: BTreeSet<ArtifactLocation>,
}

/// Thread-safe model metadata and lifecycle registry.
#[derive(Debug, Default)]
pub struct ModelRegistry {
    models: RwLock<BTreeMap<ModelId, ModelRecord>>,
}

impl ModelRegistry {
    /// Creates an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one new model and rejects duplicate IDs.
    pub fn register(
        &self,
        descriptor: ModelDescriptor,
        locations: impl IntoIterator<Item = ArtifactLocation>,
    ) -> AiResult<()> {
        descriptor.validate()?;
        let locations = locations.into_iter().collect::<BTreeSet<_>>();
        let state = if locations.is_empty() {
            ModelState::Discovered
        } else {
            ModelState::Available
        };
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        if models.contains_key(&descriptor.id) {
            return Err(AiError::Conflict("model id"));
        }
        if models.len() >= MAX_REGISTERED_MODELS {
            return Err(AiError::Capacity("model registry"));
        }
        models.insert(
            descriptor.id.clone(),
            ModelRecord {
                descriptor,
                state,
                locations,
            },
        );
        Ok(())
    }

    /// Returns a cloned record without holding a registry lock.
    pub fn get(&self, id: &ModelId) -> AiResult<ModelRecord> {
        self.models
            .read()
            .map_err(|_| AiError::InternalState)?
            .get(id)
            .cloned()
            .ok_or(AiError::NotFound("model"))
    }

    /// Returns all compatible records in stable ID order.
    pub fn candidates(&self, task: &AiTask) -> AiResult<Vec<ModelRecord>> {
        Ok(self
            .models
            .read()
            .map_err(|_| AiError::InternalState)?
            .values()
            .filter(|record| record.descriptor.supports_task(task))
            .cloned()
            .collect())
    }

    /// Applies one explicit lifecycle transition.
    pub fn transition(&self, id: &ModelId, next: ModelState) -> AiResult<()> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        if record.state != next && !valid_transition(record.state, next) {
            return Err(AiError::Conflict("model state transition"));
        }
        record.state = next;
        Ok(())
    }

    pub(crate) fn note_load_started(&self, id: &ModelId) -> AiResult<()> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        if matches!(record.state, ModelState::Available | ModelState::Failed) {
            record.state = ModelState::Loading;
        }
        Ok(())
    }

    pub(crate) fn note_load_finished(&self, id: &ModelId, success: bool) -> AiResult<()> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        if success {
            record.state = ModelState::Ready;
        } else if record.state != ModelState::Ready {
            record.state = ModelState::Failed;
        }
        Ok(())
    }

    /// Adds one location without changing artifact identity.
    pub fn add_location(&self, id: &ModelId, location: ArtifactLocation) -> AiResult<()> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        record.locations.insert(location);
        if record.state == ModelState::Discovered {
            record.state = ModelState::Available;
        }
        Ok(())
    }

    /// Removes one stale location while preserving logical identity.
    pub fn remove_location(&self, id: &ModelId, location: &ArtifactLocation) -> AiResult<bool> {
        let mut models = self.models.write().map_err(|_| AiError::InternalState)?;
        let record = models.get_mut(id).ok_or(AiError::NotFound("model"))?;
        Ok(record.locations.remove(location))
    }

    /// Returns aggregate lifecycle state without high-cardinality model labels.
    pub fn snapshot(&self) -> AiResult<ModelRegistrySnapshot> {
        let models = self.models.read().map_err(|_| AiError::InternalState)?;
        let mut snapshot = ModelRegistrySnapshot {
            registered: models.len(),
            ..ModelRegistrySnapshot::default()
        };
        for record in models.values() {
            match record.state {
                ModelState::Available => {
                    snapshot.available = snapshot.available.saturating_add(1);
                }
                ModelState::Loading => snapshot.loading = snapshot.loading.saturating_add(1),
                ModelState::Ready => snapshot.ready = snapshot.ready.saturating_add(1),
                ModelState::Failed => snapshot.failed = snapshot.failed.saturating_add(1),
                ModelState::Discovered | ModelState::Evicting => {}
            }
        }
        Ok(snapshot)
    }
}

fn valid_transition(current: ModelState, next: ModelState) -> bool {
    matches!(
        (current, next),
        (
            ModelState::Discovered,
            ModelState::Available | ModelState::Failed
        ) | (
            ModelState::Available,
            ModelState::Loading | ModelState::Failed
        ) | (ModelState::Loading, ModelState::Ready | ModelState::Failed)
            | (ModelState::Ready, ModelState::Evicting | ModelState::Failed)
            | (
                ModelState::Evicting,
                ModelState::Available | ModelState::Failed
            )
            | (ModelState::Failed, ModelState::Available)
    )
}
