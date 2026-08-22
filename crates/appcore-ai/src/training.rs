// =============================================================================
//        #######
//     ###       ###     F: training.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AdmissionDecision, AiClock, AiError, AiResourceMode, AiResult, ArtifactIdentity,
    CancellationToken, CapabilityId, HardwareProbe, ModelDescriptor, ModelId, ResourceEstimate,
    ResourceGovernor,
};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Boxed optional training operation without an executor dependency.
pub type TrainingFuture<'a, T> = Pin<Box<dyn Future<Output = AiResult<T>> + Send + 'a>>;

/// One bounded supervised linear-classification example.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TrainingExample {
    /// Non-empty input text.
    pub text: String,
    /// Zero-based class index into `TrainingJob::labels`.
    pub label: usize,
}

/// Bounded random-access dataset boundary; implementations must not hide downloads.
pub trait TrainingDataset: Send + Sync {
    /// Exact number of available examples.
    fn len(&self) -> usize;

    /// Returns whether the dataset is empty.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Loads one bounded example.
    fn example(&self, index: usize) -> AiResult<TrainingExample>;
}

/// In-memory dataset useful for explicit small jobs and deterministic tests.
#[derive(Clone, Debug)]
pub struct InMemoryTrainingDataset {
    examples: Vec<TrainingExample>,
}

impl InMemoryTrainingDataset {
    /// Validates example count, text size and non-empty inputs.
    pub fn new(
        examples: Vec<TrainingExample>,
        max_examples: usize,
        max_text_bytes: usize,
    ) -> AiResult<Self> {
        if examples.is_empty()
            || examples.len() > max_examples
            || examples
                .iter()
                .any(|example| example.text.is_empty() || example.text.len() > max_text_bytes)
        {
            return Err(AiError::InvalidInput("training dataset"));
        }
        Ok(Self { examples })
    }
}

impl TrainingDataset for InMemoryTrainingDataset {
    fn len(&self) -> usize {
        self.examples.len()
    }

    fn example(&self, index: usize) -> AiResult<TrainingExample> {
        self.examples
            .get(index)
            .cloned()
            .ok_or(AiError::NotFound("training example"))
    }
}

/// Atomic checkpoint frequency and count bounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TrainingCheckpointPolicy {
    /// Epoch frequency; zero disables intermediate checkpoints.
    pub every_epochs: usize,
    /// Maximum intermediate checkpoints written by one job.
    pub max_checkpoints: usize,
}

impl Default for TrainingCheckpointPolicy {
    fn default() -> Self {
        Self {
            every_epochs: 1,
            max_checkpoints: 3,
        }
    }
}

/// Complete bounded job for the optional Candle linear trainer.
#[derive(Clone, Debug)]
pub struct TrainingJob {
    /// Stable job identity used only for correlation.
    pub id: CapabilityId,
    /// Output logical model identity.
    pub model: ModelId,
    /// Bounded output revision.
    pub revision: String,
    /// Ordered class labels.
    pub labels: Vec<String>,
    /// Deterministic feature-vector width.
    pub input_dimensions: usize,
    /// Epoch ceiling.
    pub epochs: usize,
    /// Global optimizer-step ceiling.
    pub max_steps: usize,
    /// Requested batch size, further reduced in conservative modes.
    pub batch_size: usize,
    /// SGD learning rate.
    pub learning_rate: f64,
    /// Reproducible initialization seed.
    pub seed: u64,
    /// Training-specific peak resource estimate.
    pub resource_requirements: ResourceEstimate,
    /// Resource-governor mode for this job.
    pub resource_mode: AiResourceMode,
    /// Atomic checkpoint policy.
    pub checkpoints: TrainingCheckpointPolicy,
    /// Optional verified artifact from which training resumes.
    pub resume: Option<ArtifactIdentity>,
    /// Optional publisher provenance on outputs.
    pub publisher: Option<CapabilityId>,
    /// Maximum input bytes accepted from each example.
    pub max_input_bytes: usize,
    /// Maximum output bytes declared on the resulting descriptor.
    pub max_output_bytes: usize,
}

/// Bounded progress update emitted after an optimizer step or checkpoint.
#[derive(Clone, Debug, PartialEq)]
pub struct TrainingProgress {
    /// One-based epoch.
    pub epoch: usize,
    /// Global optimizer step.
    pub step: usize,
    /// Finite scalar loss.
    pub loss: f32,
    /// Newly written checkpoint identity when applicable.
    pub checkpoint: Option<ArtifactIdentity>,
}

/// Synchronous observer called outside Runtime locks.
pub trait TrainingProgressObserver: Send + Sync {
    /// Receives one bounded progress event.
    fn report(&self, progress: &TrainingProgress);
}

/// No-op observer for jobs that do not request progress delivery.
#[derive(Clone, Copy, Debug, Default)]
pub struct IgnoreTrainingProgress;

impl TrainingProgressObserver for IgnoreTrainingProgress {
    fn report(&self, _progress: &TrainingProgress) {}
}

/// Final resumable artifact and registry-ready descriptor.
#[derive(Clone, Debug, PartialEq)]
pub struct TrainingOutput {
    /// Encoded model artifact bytes.
    pub bytes: Vec<u8>,
    /// Content identity and provenance.
    pub identity: ArtifactIdentity,
    /// Descriptor suitable for explicit `ModelRegistry` registration.
    pub descriptor: ModelDescriptor,
    /// Completed epochs.
    pub completed_epochs: usize,
    /// Completed optimizer steps.
    pub completed_steps: usize,
    /// Final finite scalar loss.
    pub final_loss: f32,
}

/// Mandatory training-specific admission boundary.
pub trait TrainingAdmission: Send + Sync {
    /// Applies a job's explicit resource requirements.
    fn admit(&self, job: &TrainingJob) -> AiResult<AdmissionDecision>;

    /// Rechecks sampled pressure before a backend starts its next batch.
    fn batch_limit(&self, job: &TrainingJob, requested: usize) -> AiResult<usize> {
        match self.admit(job)? {
            AdmissionDecision::Admit { budget } if budget.pressure_limited => {
                Ok(requested.div_ceil(2).max(1))
            }
            AdmissionDecision::Admit { .. } => Ok(requested.max(1)),
            AdmissionDecision::Defer { .. } => {
                Err(AiError::Capacity("training resources deferred"))
            }
            AdmissionDecision::Reject { .. } => Err(AiError::Capacity("training resources")),
        }
    }
}

/// Resource-governor adapter using an independent training estimate.
#[derive(Debug)]
pub struct GovernorTrainingAdmission<P, C> {
    governor: ResourceGovernor<P>,
    clock: C,
}

impl<P, C> GovernorTrainingAdmission<P, C> {
    /// Connects a governor and injected clock to training admission.
    #[must_use]
    pub fn new(governor: ResourceGovernor<P>, clock: C) -> Self {
        Self { governor, clock }
    }
}

impl<P: HardwareProbe, C: AiClock> TrainingAdmission for GovernorTrainingAdmission<P, C> {
    fn admit(&self, job: &TrainingJob) -> AiResult<AdmissionDecision> {
        self.governor.admit(
            job.resource_mode,
            job.resource_requirements,
            self.clock.now_ms(),
        )
    }
}

/// Small training boundary implemented by one selected optional backend.
pub trait TrainingBackend: Send + Sync {
    /// Trains or resumes one bounded local job; distributed training is unsupported.
    fn train<'a>(
        &'a self,
        job: &'a TrainingJob,
        dataset: Arc<dyn TrainingDataset>,
        progress: Arc<dyn TrainingProgressObserver>,
        cancellation: &'a CancellationToken,
    ) -> TrainingFuture<'a, TrainingOutput>;
}
