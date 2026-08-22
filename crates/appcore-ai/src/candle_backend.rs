// =============================================================================
//        #######
//     ###       ###     F: candle_backend.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiError, AiLimits, AiModality, AiOutput, AiRequest, AiResponse, AiResult, AiScore, AiTask,
    ArtifactDigest, ArtifactFormat, ArtifactStore, BackendCostHints, BackendDescriptor,
    BackendDevice, BackendFuture, BackendHealth, BackendId, CancellationToken, DeviceId,
    DeviceKind, InferenceBackend, ModelDescriptor, ModelId, NativeLinearArtifact, PlacementMetrics,
    ResourceEstimate, ResourceEstimateBreakdown,
};
use candle_core::{Device, Tensor};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Instant;

#[path = "candle_batch.rs"]
mod batch;

/// Stable identity of the initial Candle CPU backend.
pub const CANDLE_LINEAR_BACKEND_ID: &str = "candle/cpu-linear-v1";
/// Maximum items accepted by one vectorized Candle classifier batch.
pub const CANDLE_LINEAR_MAX_BATCH_SIZE: usize = 64;

/// Explicit bounds for the optional Candle backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandleBackendConfig {
    /// Maximum simultaneously loaded models.
    pub max_loaded_models: usize,
    /// Maximum deterministic input dimensions.
    pub max_input_dimensions: usize,
    /// Maximum classifier labels.
    pub max_classes: usize,
    /// Maximum artifact bytes read from a store.
    pub max_artifact_bytes: u64,
    /// Maximum text bytes converted into features.
    pub max_input_bytes: usize,
    /// Peak CPU estimate submitted to admission.
    pub estimated_cpu_percent: u8,
    /// Response validation bounds.
    pub response_limits: AiLimits,
}

impl CandleBackendConfig {
    /// Validates backend-owned bounds.
    pub fn validate(self) -> AiResult<Self> {
        if self.max_loaded_models == 0
            || self.max_input_dimensions == 0
            || self.max_input_dimensions > 65_536
            || self.max_classes == 0
            || self.max_classes > 4_096
            || self.max_artifact_bytes == 0
            || self.max_input_bytes == 0
            || self.estimated_cpu_percent == 0
            || self.estimated_cpu_percent > 100
        {
            return Err(AiError::InvalidInput("Candle backend configuration"));
        }
        Ok(self)
    }
}

impl Default for CandleBackendConfig {
    fn default() -> Self {
        Self {
            max_loaded_models: 8,
            max_input_dimensions: 4_096,
            max_classes: 256,
            max_artifact_bytes: 64 * 1024 * 1024,
            max_input_bytes: 1024 * 1024,
            estimated_cpu_percent: 75,
            response_limits: AiLimits::default(),
        }
    }
}

struct LoadedLinear {
    digest: ArtifactDigest,
    input_dimensions: usize,
    labels: Vec<String>,
    weights: Tensor,
    biases: Tensor,
}

impl Debug for LoadedLinear {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("LoadedLinear")
            .field("digest", &self.digest)
            .field("input_dimensions", &self.input_dimensions)
            .field("labels", &self.labels.len())
            .finish_non_exhaustive()
    }
}

/// CPU-only Candle adapter for the bounded `NativeLinearV1` artifact format.
pub struct CandleBackend {
    descriptor: BackendDescriptor,
    store: Arc<dyn ArtifactStore>,
    config: CandleBackendConfig,
    loaded: RwLock<BTreeMap<ModelId, Arc<LoadedLinear>>>,
    active: AtomicUsize,
    inference_count: AtomicU64,
    latency_ema_ms: AtomicU64,
}

impl Debug for CandleBackend {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CandleBackend")
            .field("descriptor", &self.descriptor)
            .field("config", &self.config)
            .field(
                "loaded",
                &self.loaded.read().map(|models| models.len()).ok(),
            )
            .finish_non_exhaustive()
    }
}

impl CandleBackend {
    /// Creates a backend without loading or downloading a model.
    pub fn new(store: Arc<dyn ArtifactStore>, config: CandleBackendConfig) -> AiResult<Self> {
        let config = config.validate()?;
        Ok(Self {
            descriptor: BackendDescriptor {
                id: BackendId::new(CANDLE_LINEAR_BACKEND_ID)?,
                tasks: vec![AiTask::ClassifyText],
                input_modalities: vec![AiModality::Text],
                formats: vec![ArtifactFormat::NativeLinearV1],
                devices: vec![BackendDevice {
                    id: DeviceId::new("local/cpu/candle")?,
                    kind: DeviceKind::Cpu,
                }],
                costs: BackendCostHints {
                    load_units: 20,
                    inference_units: 2,
                    supports_batching: true,
                },
            },
            store,
            config,
            loaded: RwLock::new(BTreeMap::new()),
            active: AtomicUsize::new(0),
            inference_count: AtomicU64::new(0),
            latency_ema_ms: AtomicU64::new(0),
        })
    }

    fn load_sync(&self, model: &ModelDescriptor, cancellation: &CancellationToken) -> AiResult<()> {
        self.check_model(model)?;
        if model.artifact.signature_required && !self.store.provenance_verified(&model.artifact)? {
            return Err(AiError::Integrity("artifact signature not verified"));
        }
        if let Some(existing) = self
            .loaded
            .read()
            .map_err(|_| AiError::InternalState)?
            .get(&model.id)
        {
            return if existing.digest == model.artifact.digest {
                Ok(())
            } else {
                Err(AiError::Conflict("loaded model revision"))
            };
        }
        let bytes = self.store.load(
            &model.artifact,
            self.config.max_artifact_bytes,
            cancellation,
        )?;
        let artifact = NativeLinearArtifact::decode(
            &bytes,
            self.config.max_input_dimensions,
            self.config.max_classes,
        )?;
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        let class_count = artifact.labels().len();
        let device = Device::Cpu;
        let weights = Tensor::from_vec(
            artifact.weights().to_vec(),
            (class_count, artifact.input_dimensions()),
            &device,
        )
        .map_err(|_| self.failure("tensor-weights"))?;
        let biases = Tensor::from_vec(artifact.biases().to_vec(), class_count, &device)
            .map_err(|_| self.failure("tensor-biases"))?;
        let loaded = Arc::new(LoadedLinear {
            digest: model.artifact.digest,
            input_dimensions: artifact.input_dimensions(),
            labels: artifact.labels().to_vec(),
            weights,
            biases,
        });
        let mut models = self.loaded.write().map_err(|_| AiError::InternalState)?;
        if models.len() >= self.config.max_loaded_models {
            return Err(AiError::Capacity("Candle loaded models"));
        }
        match models.get(&model.id) {
            Some(existing) if existing.digest == model.artifact.digest => Ok(()),
            Some(_) => Err(AiError::Conflict("loaded model revision")),
            None => {
                models.insert(model.id.clone(), loaded);
                Ok(())
            }
        }
    }

    fn infer_sync(
        &self,
        request: &AiRequest,
        model: &ModelDescriptor,
        cancellation: &CancellationToken,
    ) -> AiResult<AiResponse> {
        check_cancellation(cancellation)?;
        let text = self.request_text(request)?;
        let loaded = self.loaded_model(model)?;
        let started = Instant::now();
        let _active = ActiveInference::new(&self.active);
        let features = text_features(text, loaded.input_dimensions);
        let input = Tensor::from_vec(features, (1, loaded.input_dimensions), &Device::Cpu)
            .map_err(|_| self.failure("tensor-input"))?;
        let transposed = loaded
            .weights
            .t()
            .map_err(|_| self.failure("tensor-transpose"))?;
        let logits = input
            .matmul(&transposed)
            .and_then(|value| value.broadcast_add(&loaded.biases))
            .map_err(|_| self.failure("linear-inference"))?;
        let probabilities = candle_nn::ops::softmax_last_dim(&logits)
            .and_then(|value| value.to_vec2::<f32>())
            .map_err(|_| self.failure("linear-softmax"))?
            .into_iter()
            .next()
            .ok_or_else(|| self.failure("linear-empty-output"))?;
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        self.inference_count.fetch_add(1, Ordering::Relaxed);
        update_ema(&self.latency_ema_ms, elapsed_ms(started));
        self.response(model, &loaded, probabilities)
    }

    fn request_text<'a>(&self, request: &'a AiRequest) -> AiResult<&'a str> {
        if request.task != AiTask::ClassifyText {
            return Err(AiError::Incompatible("Candle linear task"));
        }
        let text = request
            .input
            .single_text()
            .ok_or(AiError::InvalidInput("Candle linear text input"))?;
        if text.len() > self.config.max_input_bytes {
            return Err(AiError::LimitExceeded {
                kind: crate::LimitKind::InputBytes,
                actual: u64::try_from(text.len()).unwrap_or(u64::MAX),
                limit: u64::try_from(self.config.max_input_bytes).unwrap_or(u64::MAX),
            });
        }
        Ok(text)
    }

    fn loaded_model(&self, model: &ModelDescriptor) -> AiResult<Arc<LoadedLinear>> {
        let loaded = self
            .loaded
            .read()
            .map_err(|_| AiError::InternalState)?
            .get(&model.id)
            .cloned()
            .ok_or(AiError::BackendUnavailable(self.descriptor.id.clone()))?;
        if loaded.digest != model.artifact.digest {
            return Err(AiError::Conflict("loaded model revision"));
        }
        Ok(loaded)
    }

    fn response(
        &self,
        model: &ModelDescriptor,
        loaded: &LoadedLinear,
        probabilities: Vec<f32>,
    ) -> AiResult<AiResponse> {
        let scores = loaded
            .labels
            .iter()
            .cloned()
            .zip(probabilities)
            .map(|(label, score)| AiScore { label, score })
            .collect::<Vec<_>>();
        let output_bytes = scores.iter().fold(0usize, |sum, score| {
            sum.saturating_add(score.label.len() + 4)
        });
        if output_bytes > model.max_output_bytes {
            return Err(AiError::LimitExceeded {
                kind: crate::LimitKind::OutputBytes,
                actual: u64::try_from(output_bytes).unwrap_or(u64::MAX),
                limit: u64::try_from(model.max_output_bytes).unwrap_or(u64::MAX),
            });
        }
        AiResponse::new(
            AiOutput::Scores(scores),
            Vec::new(),
            None,
            self.config.response_limits,
        )
    }

    fn check_model(&self, model: &ModelDescriptor) -> AiResult<()> {
        if model.format != ArtifactFormat::NativeLinearV1
            || !model.supports_task(&AiTask::ClassifyText)
            || !model.supports_route(&self.descriptor.id, DeviceKind::Cpu)
            || model.artifact.size_bytes > self.config.max_artifact_bytes
        {
            return Err(AiError::Incompatible("Candle linear model"));
        }
        Ok(())
    }

    fn failure(&self, code: &'static str) -> AiError {
        AiError::BackendFailure {
            backend: self.descriptor.id.clone(),
            code,
        }
    }
}

impl InferenceBackend for CandleBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn health(&self) -> BackendHealth {
        BackendHealth::Healthy
    }

    fn placement_metrics(&self, _device: &DeviceId) -> AiResult<PlacementMetrics> {
        let active = self.active.load(Ordering::Relaxed);
        Ok(PlacementMetrics {
            load_percent: Some(u8::try_from(active.saturating_mul(25).min(100)).unwrap_or(100)),
            queue_depth: 0,
            available_memory_bytes: None,
            available_vram_bytes: Some(0),
            latency_ema_ms: match self.latency_ema_ms.load(Ordering::Relaxed) {
                0 => None,
                value => Some(value),
            },
            throughput_ema: None,
        })
    }

    fn estimate(
        &self,
        _request: &AiRequest,
        model: &ModelDescriptor,
        device: &DeviceId,
    ) -> AiResult<ResourceEstimate> {
        if device != &self.descriptor.devices[0].id {
            return Err(AiError::Incompatible("Candle device"));
        }
        let peak_memory = model
            .estimated_memory_bytes
            .max(model.artifact.size_bytes.saturating_mul(2));
        Ok(ResourceEstimateBreakdown {
            cpu_percent: self.config.estimated_cpu_percent,
            gpu_percent: 0,
            model_memory_bytes: model.artifact.size_bytes,
            runtime_memory_bytes: peak_memory.saturating_sub(model.artifact.size_bytes),
            workers: 1,
            ..ResourceEstimateBreakdown::default()
        }
        .peak())
    }

    fn load<'a>(
        &'a self,
        model: &'a ModelDescriptor,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move { self.load_sync(model, cancellation) })
    }

    fn unload<'a>(
        &'a self,
        model: &'a ModelDescriptor,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(AiError::Cancelled);
            }
            self.loaded
                .write()
                .map_err(|_| AiError::InternalState)?
                .remove(&model.id);
            Ok(())
        })
    }

    fn infer<'a>(
        &'a self,
        request: &'a AiRequest,
        model: &'a ModelDescriptor,
        device: &'a DeviceId,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, AiResponse> {
        Box::pin(async move {
            if device != &self.descriptor.devices[0].id {
                return Err(AiError::Incompatible("Candle device"));
            }
            self.infer_sync(request, model, cancellation)
        })
    }

    fn infer_batch<'a>(
        &'a self,
        requests: &'a [AiRequest],
        model: &'a ModelDescriptor,
        device: &'a DeviceId,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, Vec<AiResult<AiResponse>>> {
        Box::pin(async move {
            if device != &self.descriptor.devices[0].id {
                return Err(AiError::Incompatible("Candle device"));
            }
            self.infer_batch_sync(requests, model, cancellation)
        })
    }
}

struct ActiveInference<'a> {
    active: &'a AtomicUsize,
}

impl<'a> ActiveInference<'a> {
    fn new(active: &'a AtomicUsize) -> Self {
        active.fetch_add(1, Ordering::Relaxed);
        Self { active }
    }
}

impl Drop for ActiveInference<'_> {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }
}

fn text_features(text: &str, dimensions: usize) -> Vec<f32> {
    let mut features = vec![0.0; dimensions];
    for (index, byte) in text.bytes().enumerate() {
        let slot = (index.saturating_mul(257) ^ usize::from(byte)) % dimensions;
        features[slot] += 1.0;
    }
    let divisor = text.len().max(1) as f32;
    for value in &mut features {
        *value /= divisor;
    }
    features
}

fn check_cancellation(cancellation: &CancellationToken) -> AiResult<()> {
    if cancellation.is_cancelled() {
        Err(AiError::Cancelled)
    } else {
        Ok(())
    }
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis())
        .unwrap_or(u64::MAX)
        .max(1)
}

fn update_ema(target: &AtomicU64, sample: u64) {
    let mut previous = target.load(Ordering::Relaxed);
    loop {
        let next = if previous == 0 {
            sample
        } else {
            previous.saturating_mul(4).saturating_add(sample) / 5
        };
        match target.compare_exchange_weak(previous, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(observed) => previous = observed,
        }
    }
}
