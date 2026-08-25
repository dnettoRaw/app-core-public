// =============================================================================
//        #######
//     ###       ###     F: backend.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiModality, AiRequest, AiResponse, AiResult, AiStreamSink, AiTask, ArtifactFormat,
    BackendId, CancellationToken, DeviceId, DeviceKind, ModelDescriptor, PlacementMetrics,
    ResourceEstimate,
};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};

const MAX_REGISTERED_BACKENDS: usize = 256;

/// Boxed asynchronous backend operation without an executor dependency.
pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = AiResult<T>> + Send + 'a>>;

/// Coarse backend availability used by routing and diagnostics.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendHealth {
    /// Backend accepts normal work.
    Healthy,
    /// Backend accepts work but should lose equal-cost placement ties.
    Degraded,
    /// Backend must not receive new work.
    Unavailable,
}

/// Low-cardinality registry health summary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BackendRegistrySnapshot {
    /// Registered adapters.
    pub registered: usize,
    /// Adapters accepting normal work.
    pub healthy: usize,
    /// Adapters accepting work with degraded preference.
    pub degraded: usize,
    /// Adapters rejecting new work.
    pub unavailable: usize,
}

/// One backend-owned compute device exposed through stable AppCore types.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendDevice {
    /// Stable device identity.
    pub id: DeviceId,
    /// Backend-neutral hardware class.
    pub kind: DeviceKind,
}

/// Backend-neutral relative cost hints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BackendCostHints {
    /// Relative cold load cost.
    pub load_units: u64,
    /// Relative cost per inference item.
    pub inference_units: u64,
    /// Whether compatible requests may be batched.
    pub supports_batching: bool,
}

/// Immutable capabilities declared by an inference engine.
#[derive(Clone, Debug, PartialEq)]
pub struct BackendDescriptor {
    /// Stable backend identity.
    pub id: BackendId,
    /// Supported tasks.
    pub tasks: Vec<AiTask>,
    /// Input modalities the adapter can safely transport and decode.
    pub input_modalities: Vec<AiModality>,
    /// Supported artifact formats.
    pub formats: Vec<ArtifactFormat>,
    /// Available devices.
    pub devices: Vec<BackendDevice>,
    /// Relative cost hints.
    pub costs: BackendCostHints,
}

impl BackendDescriptor {
    /// Validates bounded declarations before backend registration.
    pub fn validate(&self) -> AiResult<()> {
        if self.tasks.is_empty()
            || self.tasks.len() > 32
            || self.input_modalities.is_empty()
            || self.input_modalities.len() > 8
            || self.formats.is_empty()
            || self.formats.len() > 16
            || self.devices.is_empty()
            || self.devices.len() > 32
        {
            return Err(AiError::InvalidInput("backend descriptor bounds"));
        }
        Ok(())
    }

    /// Returns the first compatible device, honoring an optional forced ID.
    #[must_use]
    pub fn compatible_device(
        &self,
        model: &ModelDescriptor,
        forced: Option<&DeviceId>,
    ) -> Option<&BackendDevice> {
        self.devices.iter().find(|device| {
            forced.is_none_or(|required| required == &device.id)
                && model.supported_devices.contains(&device.kind)
        })
    }

    /// Returns every compatible device in stable declaration order.
    #[must_use]
    pub fn compatible_devices<'a>(
        &'a self,
        model: &ModelDescriptor,
        forced: Option<&DeviceId>,
    ) -> Vec<&'a BackendDevice> {
        self.devices
            .iter()
            .filter(|device| {
                forced.is_none_or(|required| required == &device.id)
                    && model.supported_devices.contains(&device.kind)
            })
            .collect()
    }
}

/// Backend SPI for model lifecycle and inference.
pub trait InferenceBackend: Send + Sync {
    /// Returns immutable backend capabilities.
    fn descriptor(&self) -> &BackendDescriptor;

    /// Returns current backend health without exposing provider diagnostics.
    fn health(&self) -> BackendHealth;

    /// Reports bounded recent load, queue, capacity and EMA observations.
    fn placement_metrics(&self, _device: &DeviceId) -> AiResult<PlacementMetrics> {
        Ok(PlacementMetrics::default())
    }

    /// Estimates peak resources for one concrete request and device.
    fn estimate(
        &self,
        request: &AiRequest,
        model: &ModelDescriptor,
        device: &DeviceId,
    ) -> AiResult<ResourceEstimate>;

    /// Loads a verified model artifact into backend-owned state.
    fn load<'a>(
        &'a self,
        model: &'a ModelDescriptor,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()>;

    /// Unloads backend-owned model state.
    fn unload<'a>(
        &'a self,
        model: &'a ModelDescriptor,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()>;

    /// Executes one validated request on the selected device.
    fn infer<'a>(
        &'a self,
        request: &'a AiRequest,
        model: &'a ModelDescriptor,
        device: &'a DeviceId,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, AiResponse>;

    /// Streams bounded deltas with synchronous sink backpressure.
    ///
    /// Backends without native streaming emit one complete response event after
    /// inference, preserving the same cancellation and output validation.
    fn infer_stream<'a>(
        &'a self,
        request: &'a AiRequest,
        model: &'a ModelDescriptor,
        device: &'a DeviceId,
        cancellation: &'a CancellationToken,
        sink: &'a dyn AiStreamSink,
    ) -> BackendFuture<'a, AiResponse> {
        Box::pin(async move {
            let response = self.infer(request, model, device, cancellation).await?;
            crate::streaming::emit_complete(&response, cancellation, sink)?;
            Ok(response)
        })
    }

    /// Executes a compatible batch with independent per-item failure semantics.
    fn infer_batch<'a>(
        &'a self,
        requests: &'a [AiRequest],
        model: &'a ModelDescriptor,
        device: &'a DeviceId,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, Vec<AiResult<AiResponse>>> {
        Box::pin(async move {
            let mut outcomes = Vec::with_capacity(requests.len());
            for request in requests {
                if cancellation.is_cancelled() {
                    outcomes.push(Err(AiError::Cancelled));
                } else {
                    outcomes.push(self.infer(request, model, device, cancellation).await);
                }
            }
            Ok(outcomes)
        })
    }
}

/// Explicit deterministic registry for inference backends.
#[derive(Default)]
pub struct BackendRegistry {
    backends: RwLock<BTreeMap<BackendId, Arc<dyn InferenceBackend>>>,
}

impl Debug for BackendRegistry {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("BackendRegistry")
            .field(
                "registered",
                &self.backends.read().map(|values| values.len()).ok(),
            )
            .finish()
    }
}

impl BackendRegistry {
    /// Creates an empty registry with no implicit backend discovery.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one backend and rejects duplicate IDs.
    pub fn register(&self, backend: Arc<dyn InferenceBackend>) -> AiResult<()> {
        backend.descriptor().validate()?;
        let id = backend.descriptor().id.clone();
        let mut backends = self.backends.write().map_err(|_| AiError::InternalState)?;
        if backends.contains_key(&id) {
            return Err(AiError::Conflict("backend id"));
        }
        if backends.len() >= MAX_REGISTERED_BACKENDS {
            return Err(AiError::Capacity("backend registry"));
        }
        backends.insert(id, backend);
        Ok(())
    }

    /// Returns one explicitly named backend.
    pub fn get(&self, id: &BackendId) -> AiResult<Arc<dyn InferenceBackend>> {
        self.backends
            .read()
            .map_err(|_| AiError::InternalState)?
            .get(id)
            .cloned()
            .ok_or(AiError::NotFound("backend"))
    }

    /// Returns compatible healthy or degraded backends in stable ID order.
    pub fn candidates(
        &self,
        request: &AiRequest,
        model: &ModelDescriptor,
    ) -> AiResult<Vec<Arc<dyn InferenceBackend>>> {
        let modalities = request.input.modalities();
        self.candidates_with_modalities(request, model, &modalities)
    }

    pub(crate) fn candidates_with_modalities(
        &self,
        request: &AiRequest,
        model: &ModelDescriptor,
        modalities: &[AiModality],
    ) -> AiResult<Vec<Arc<dyn InferenceBackend>>> {
        let backends = self.backends.read().map_err(|_| AiError::InternalState)?;
        if !model.supports_modalities(modalities) {
            return Ok(Vec::new());
        }
        if let Some(required) = &request.options.backend {
            if !model.supported_backends.contains(required) {
                return Ok(Vec::new());
            }
            return Ok(backends
                .get(required)
                .filter(|backend| compatible(backend.as_ref(), request, model, modalities))
                .cloned()
                .into_iter()
                .collect());
        }
        let mut supported = model.supported_backends.iter().collect::<Vec<_>>();
        supported.sort_unstable();
        supported.dedup();
        Ok(supported
            .into_iter()
            .filter_map(|id| backends.get(id))
            .filter(|backend| compatible(backend.as_ref(), request, model, modalities))
            .cloned()
            .collect())
    }

    /// Returns an aggregate health snapshot without backend IDs as labels.
    pub fn snapshot(&self) -> AiResult<BackendRegistrySnapshot> {
        let backends = self.backends.read().map_err(|_| AiError::InternalState)?;
        let mut snapshot = BackendRegistrySnapshot {
            registered: backends.len(),
            ..BackendRegistrySnapshot::default()
        };
        for backend in backends.values() {
            match backend.health() {
                BackendHealth::Healthy => snapshot.healthy = snapshot.healthy.saturating_add(1),
                BackendHealth::Degraded => {
                    snapshot.degraded = snapshot.degraded.saturating_add(1);
                }
                BackendHealth::Unavailable => {
                    snapshot.unavailable = snapshot.unavailable.saturating_add(1);
                }
            }
        }
        Ok(snapshot)
    }
}

fn compatible(
    backend: &dyn InferenceBackend,
    request: &AiRequest,
    model: &ModelDescriptor,
    modalities: &[AiModality],
) -> bool {
    let descriptor = backend.descriptor();
    backend.health() != BackendHealth::Unavailable
        && descriptor.tasks.contains(&request.task)
        && modalities
            .iter()
            .all(|modality| descriptor.input_modalities.contains(modality))
        && descriptor.formats.contains(&model.format)
        && descriptor
            .compatible_device(model, request.options.device.as_ref())
            .is_some()
}
