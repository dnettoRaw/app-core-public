// =============================================================================
//        #######
//     ###       ###     F: candle_training.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AdmissionDecision, AiError, AiModality, AiResult, AiTask, ArtifactFormat, ArtifactStore,
    BackendId, CancellationToken, DeviceKind, ModelDescriptor, NativeLinearArtifact, QualityTier,
    Quantization, TrainingAdmission, TrainingBackend, TrainingDataset, TrainingFuture, TrainingJob,
    TrainingOutput, TrainingProgress, TrainingProgressObserver, CANDLE_LINEAR_BACKEND_ID,
};
use candle_core::{Device, Tensor, Var};
use candle_nn::Optimizer;
use std::sync::Arc;

/// Backend-owned ceilings for optional local Candle training.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CandleTrainerConfig {
    /// Maximum dataset examples.
    pub max_examples: usize,
    /// Maximum feature dimensions.
    pub max_input_dimensions: usize,
    /// Maximum class count.
    pub max_classes: usize,
    /// Maximum epochs.
    pub max_epochs: usize,
    /// Maximum optimizer steps.
    pub max_steps: usize,
    /// Maximum batch size.
    pub max_batch_size: usize,
    /// Maximum encoded checkpoint bytes.
    pub max_artifact_bytes: u64,
}

impl CandleTrainerConfig {
    /// Validates every hard backend limit.
    pub fn validate(self) -> AiResult<Self> {
        if self.max_examples == 0
            || self.max_input_dimensions == 0
            || self.max_input_dimensions > 65_536
            || self.max_classes < 2
            || self.max_classes > 4_096
            || self.max_epochs == 0
            || self.max_steps == 0
            || self.max_batch_size == 0
            || self.max_artifact_bytes == 0
        {
            return Err(AiError::InvalidInput("Candle trainer configuration"));
        }
        Ok(self)
    }
}

impl Default for CandleTrainerConfig {
    fn default() -> Self {
        Self {
            max_examples: 100_000,
            max_input_dimensions: 4_096,
            max_classes: 256,
            max_epochs: 100,
            max_steps: 100_000,
            max_batch_size: 512,
            max_artifact_bytes: 64 * 1024 * 1024,
        }
    }
}

/// Local, non-distributed Candle SGD trainer for `NativeLinearV1`.
pub struct CandleTrainer {
    store: Arc<dyn ArtifactStore>,
    admission: Arc<dyn TrainingAdmission>,
    config: CandleTrainerConfig,
}

impl std::fmt::Debug for CandleTrainer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CandleTrainer")
            .field("config", &self.config)
            .finish_non_exhaustive()
    }
}

impl CandleTrainer {
    /// Creates a trainer with mandatory resource admission and artifact storage.
    pub fn new(
        store: Arc<dyn ArtifactStore>,
        admission: Arc<dyn TrainingAdmission>,
        config: CandleTrainerConfig,
    ) -> AiResult<Self> {
        Ok(Self {
            store,
            admission,
            config: config.validate()?,
        })
    }

    fn train_sync(
        &self,
        job: &TrainingJob,
        dataset: &dyn TrainingDataset,
        progress: &dyn TrainingProgressObserver,
        cancellation: &CancellationToken,
    ) -> AiResult<TrainingOutput> {
        self.validate(job, dataset)?;
        match self.admission.admit(job)? {
            AdmissionDecision::Admit { .. } => {}
            AdmissionDecision::Defer { .. } | AdmissionDecision::Reject { .. } => {
                return Err(AiError::Capacity("Candle training admission"));
            }
        }
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        let device = Device::Cpu;
        let (weights, biases) = self.initial_variables(job, cancellation, &device)?;
        let mut optimizer =
            candle_nn::SGD::new(vec![weights.clone(), biases.clone()], job.learning_rate)
                .map_err(|_| failure("optimizer"))?;
        let maximum_batch_size = effective_batch_size(job, self.config).min(dataset.len());
        let mut completed_steps = 0usize;
        let mut completed_epochs = 0usize;
        let mut checkpoints = 0usize;
        let mut final_loss = None;
        for epoch in 1..=job.epochs {
            if completed_steps >= job.max_steps {
                break;
            }
            let mut start = 0usize;
            while start < dataset.len() {
                if cancellation.is_cancelled() {
                    return Err(AiError::Cancelled);
                }
                if completed_steps >= job.max_steps {
                    break;
                }
                let batch_size = self
                    .admission
                    .batch_limit(job, maximum_batch_size)?
                    .clamp(1, maximum_batch_size);
                let count = batch_size.min(dataset.len().saturating_sub(start));
                let (features, targets) = load_batch(job, dataset, start, count)?;
                let inputs = Tensor::from_vec(features, (count, job.input_dimensions), &device)
                    .map_err(|_| AiError::Capacity("Candle training input tensor"))?;
                let targets = Tensor::from_vec(targets, count, &device)
                    .map_err(|_| AiError::Capacity("Candle training target tensor"))?;
                let transposed = weights.t().map_err(|_| failure("training-transpose"))?;
                let logits = inputs
                    .matmul(&transposed)
                    .and_then(|value| value.broadcast_add(&biases))
                    .map_err(|_| failure("training-forward"))?;
                let loss = candle_nn::loss::cross_entropy(&logits, &targets)
                    .map_err(|_| failure("training-loss"))?;
                let scalar = loss
                    .to_scalar::<f32>()
                    .map_err(|_| failure("training-loss-scalar"))?;
                if !scalar.is_finite() {
                    return Err(failure("training-non-finite"));
                }
                optimizer
                    .backward_step(&loss)
                    .map_err(|_| failure("training-backward"))?;
                completed_steps = completed_steps.saturating_add(1);
                final_loss = Some(scalar);
                progress.report(&TrainingProgress {
                    epoch,
                    step: completed_steps,
                    loss: scalar,
                    checkpoint: None,
                });
                start = start.saturating_add(count);
            }
            completed_epochs = epoch;
            if should_checkpoint(job, epoch, checkpoints) {
                let checkpoint = build_artifact(job, &weights, &biases)?;
                let identity = checkpoint.identity(job.publisher.clone(), false)?;
                let bytes = checkpoint.encode()?;
                self.checkpoint(&identity, &bytes, cancellation)?;
                checkpoints = checkpoints.saturating_add(1);
                progress.report(&TrainingProgress {
                    epoch,
                    step: completed_steps,
                    loss: final_loss.unwrap_or_default(),
                    checkpoint: Some(identity),
                });
            }
        }
        let final_loss = final_loss.ok_or(AiError::InvalidInput("empty training execution"))?;
        let artifact = build_artifact(job, &weights, &biases)?;
        let bytes = artifact.encode()?;
        let identity = artifact.identity(job.publisher.clone(), false)?;
        self.checkpoint(&identity, &bytes, cancellation)?;
        let descriptor = output_descriptor(job, identity.clone(), bytes.len())?;
        Ok(TrainingOutput {
            bytes,
            identity,
            descriptor,
            completed_epochs,
            completed_steps,
            final_loss,
        })
    }

    fn initial_variables(
        &self,
        job: &TrainingJob,
        cancellation: &CancellationToken,
        device: &Device,
    ) -> AiResult<(Var, Var)> {
        let artifact = if let Some(identity) = &job.resume {
            if identity.signature_required {
                return Err(AiError::Integrity("resume signature not verified"));
            }
            let bytes = self
                .store
                .load(identity, self.config.max_artifact_bytes, cancellation)?;
            let artifact = NativeLinearArtifact::decode(
                &bytes,
                self.config.max_input_dimensions,
                self.config.max_classes,
            )?;
            if artifact.input_dimensions() != job.input_dimensions
                || artifact.labels() != job.labels
            {
                return Err(AiError::Incompatible("training resume artifact"));
            }
            artifact
        } else {
            initialized_artifact(job)?
        };
        let weights = Var::from_vec(
            artifact.weights().to_vec(),
            (job.labels.len(), job.input_dimensions),
            device,
        )
        .map_err(|_| AiError::Capacity("Candle training weights"))?;
        let biases = Var::from_vec(artifact.biases().to_vec(), job.labels.len(), device)
            .map_err(|_| AiError::Capacity("Candle training biases"))?;
        Ok((weights, biases))
    }

    fn checkpoint(
        &self,
        identity: &crate::ArtifactIdentity,
        bytes: &[u8],
        cancellation: &CancellationToken,
    ) -> AiResult<()> {
        if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > self.config.max_artifact_bytes {
            return Err(AiError::Capacity("Candle training artifact"));
        }
        self.store.store(identity, bytes, cancellation)
    }

    fn validate(&self, job: &TrainingJob, dataset: &dyn TrainingDataset) -> AiResult<()> {
        if dataset.is_empty()
            || dataset.len() > self.config.max_examples
            || job.labels.len() < 2
            || job.labels.len() > self.config.max_classes
            || job
                .labels
                .iter()
                .any(|label| label.is_empty() || label.len() > 96)
            || job.input_dimensions == 0
            || job.input_dimensions > self.config.max_input_dimensions
            || job.epochs == 0
            || job.epochs > self.config.max_epochs
            || job.max_steps == 0
            || job.max_steps > self.config.max_steps
            || job.batch_size == 0
            || job.batch_size > self.config.max_batch_size
            || !job.learning_rate.is_finite()
            || job.learning_rate <= 0.0
            || job.max_input_bytes == 0
            || job.max_output_bytes == 0
            || job.resource_requirements.workers == 0
            || job.checkpoints.max_checkpoints > self.config.max_epochs
            || job.revision.is_empty()
            || job.revision.len() > 96
        {
            return Err(AiError::InvalidInput("Candle training job"));
        }
        Ok(())
    }
}

impl TrainingBackend for CandleTrainer {
    fn train<'a>(
        &'a self,
        job: &'a TrainingJob,
        dataset: Arc<dyn TrainingDataset>,
        progress: Arc<dyn TrainingProgressObserver>,
        cancellation: &'a CancellationToken,
    ) -> TrainingFuture<'a, TrainingOutput> {
        Box::pin(
            async move { self.train_sync(job, dataset.as_ref(), progress.as_ref(), cancellation) },
        )
    }
}

fn initialized_artifact(job: &TrainingJob) -> AiResult<NativeLinearArtifact> {
    let count = job
        .input_dimensions
        .checked_mul(job.labels.len())
        .ok_or(AiError::Capacity("training weights"))?;
    let mut state = job.seed.max(1);
    let mut weights = Vec::with_capacity(count);
    for _ in 0..count {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let centered = f32::from(u16::try_from(state >> 48).unwrap_or(u16::MAX)) / 65_535.0;
        weights.push((centered - 0.5) * 0.02);
    }
    NativeLinearArtifact::new(
        job.input_dimensions,
        job.labels.clone(),
        weights,
        vec![0.0; job.labels.len()],
    )
}

fn load_batch(
    job: &TrainingJob,
    dataset: &dyn TrainingDataset,
    start: usize,
    count: usize,
) -> AiResult<(Vec<f32>, Vec<u32>)> {
    let capacity = count
        .checked_mul(job.input_dimensions)
        .ok_or(AiError::Capacity("training batch"))?;
    let mut features = Vec::with_capacity(capacity);
    let mut targets = Vec::with_capacity(count);
    for index in start..start.saturating_add(count) {
        let example = dataset.example(index)?;
        if example.text.is_empty()
            || example.text.len() > job.max_input_bytes
            || example.label >= job.labels.len()
        {
            return Err(AiError::InvalidInput("training example"));
        }
        features.extend(text_features(&example.text, job.input_dimensions));
        targets.push(
            u32::try_from(example.label).map_err(|_| AiError::InvalidInput("training label"))?,
        );
    }
    Ok((features, targets))
}

fn build_artifact(
    job: &TrainingJob,
    weights: &Var,
    biases: &Var,
) -> AiResult<NativeLinearArtifact> {
    let weights = weights
        .to_vec2::<f32>()
        .map_err(|_| failure("training-export-weights"))?
        .into_iter()
        .flatten()
        .collect();
    let biases = biases
        .to_vec1::<f32>()
        .map_err(|_| failure("training-export-biases"))?;
    NativeLinearArtifact::new(job.input_dimensions, job.labels.clone(), weights, biases)
}

fn output_descriptor(
    job: &TrainingJob,
    identity: crate::ArtifactIdentity,
    artifact_bytes: usize,
) -> AiResult<ModelDescriptor> {
    Ok(ModelDescriptor {
        id: job.model.clone(),
        revision: job.revision.clone(),
        tasks: vec![AiTask::ClassifyText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::NativeLinearV1,
        quantization: Quantization::None,
        estimated_memory_bytes: u64::try_from(artifact_bytes)
            .unwrap_or(u64::MAX)
            .saturating_mul(2),
        estimated_vram_bytes: 0,
        max_input_bytes: job.max_input_bytes,
        max_output_bytes: job.max_output_bytes,
        context_limit: None,
        supported_backends: vec![BackendId::new(CANDLE_LINEAR_BACKEND_ID)?],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 20,
        quality: Some(QualityTier::Tiny),
        artifact: identity,
    })
}

fn effective_batch_size(job: &TrainingJob, config: CandleTrainerConfig) -> usize {
    let requested = job.batch_size.min(config.max_batch_size);
    match job.resource_mode {
        crate::AiResourceMode::Eco => 1,
        crate::AiResourceMode::Balanced | crate::AiResourceMode::Custom(_) => requested.div_ceil(2),
        crate::AiResourceMode::Performance | crate::AiResourceMode::Unrestricted => requested,
    }
}

fn should_checkpoint(job: &TrainingJob, epoch: usize, completed: usize) -> bool {
    job.checkpoints.every_epochs > 0
        && completed < job.checkpoints.max_checkpoints
        && epoch.is_multiple_of(job.checkpoints.every_epochs)
}

fn text_features(text: &str, dimensions: usize) -> Vec<f32> {
    let mut features = vec![0.0; dimensions];
    for (index, byte) in text.bytes().enumerate() {
        let slot = (index.saturating_mul(257) ^ usize::from(byte)) % dimensions;
        features[slot] += 1.0;
    }
    let divisor = text.len().max(1) as f32;
    features.iter_mut().for_each(|value| *value /= divisor);
    features
}

fn failure(code: &'static str) -> AiError {
    match BackendId::new(CANDLE_LINEAR_BACKEND_ID) {
        Ok(backend) => AiError::BackendFailure { backend, code },
        Err(_) => AiError::InternalState,
    }
}
