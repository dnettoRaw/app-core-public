// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Bounded, backend-neutral AI orchestration for AppCore Runtime.
//!
//! The default build contains no machine-learning runtime. It provides
//! validated contracts, cancellation and resource admission that lightweight
//! resolvers and optional backends can share.

#![deny(unsafe_code)]
#![deny(missing_docs)]

#[cfg(all(feature = "accelerator-nvidia", any(target_os = "linux", windows)))]
mod accelerator_nvidia;
mod admission;
mod artifact;
mod artifact_metrics;
mod artifact_store;
mod backend;
mod batching;
mod bundle;
mod cancellation;
#[cfg(feature = "backend-candle")]
mod candle_backend;
#[cfg(feature = "training-candle")]
mod candle_training;
mod error;
mod execution_queue;
mod execution_route;
mod generation;
mod governor;
mod governor_policy;
mod hardware_sampler;
mod id;
mod lightweight;
#[cfg(feature = "backend-candle")]
mod linear_format;
mod modality;
mod model;
mod model_load;
mod observability;
#[cfg(feature = "backend-openai-compatible")]
mod openai_backend;
#[cfg(feature = "backend-openai-compatible")]
mod openai_blocking;
#[cfg(feature = "backend-openai-compatible")]
mod openai_codec;
#[cfg(feature = "backend-openai-compatible")]
mod openai_config;
#[cfg(feature = "backend-openai-compatible")]
mod openai_stream;
#[cfg(feature = "backend-openai-compatible")]
mod openai_transport;
mod policy;
mod queue;
mod request;
mod request_debug;
mod residency;
mod residency_metrics;
mod residency_validation;
mod resource;
#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
mod resource_fallback;
#[cfg(target_os = "linux")]
mod resource_linux;
#[cfg(target_os = "macos")]
#[allow(unsafe_code)]
mod resource_macos;
mod resource_platform;
#[cfg(windows)]
#[allow(unsafe_code)]
mod resource_windows;
mod response;
mod router;
mod router_execution;
mod router_local;
mod router_support;
mod runtime_health;
mod scheduler;
mod scheduler_score;
mod security;
mod streaming;
#[cfg(feature = "swarm")]
mod swarm;
#[cfg(feature = "training-candle")]
mod training;

pub use admission::{AiClock, GovernorAdmission, ModelAdmission, StaticAiClock, SystemAiClock};
pub use artifact::{ArtifactDigest, LocalArtifactCache};
pub use artifact_metrics::PeerArtifactMetrics;
pub use artifact_store::{
    ArtifactStore, ArtifactStoreDescriptor, ArtifactStoreKind, MemoryArtifactStore,
    PeerArtifactStore, PeerArtifactTransport, TieredArtifactStore,
};
pub use backend::{
    BackendCostHints, BackendDescriptor, BackendDevice, BackendFuture, BackendHealth,
    BackendRegistry, BackendRegistrySnapshot, InferenceBackend,
};
pub use batching::{
    BatchAdmission, BatchDispatchPolicy, BatchItem, BatchItemOutcome, BatchKey, BatchPressure,
    BatchRejectionReason, BatchTaskClass, BatcherMetrics, DynamicBatcher, DynamicBatcherConfig,
    ReadyBatch,
};
pub use bundle::{
    ArtifactBundleManifest, ArtifactSegment, ArtifactSegmentKind, LoadedArtifactSegment,
    SegmentedModelReader,
};
pub use cancellation::CancellationToken;
#[cfg(feature = "backend-candle")]
pub use candle_backend::{
    CandleBackend, CandleBackendConfig, CANDLE_LINEAR_BACKEND_ID, CANDLE_LINEAR_MAX_BATCH_SIZE,
};
#[cfg(feature = "training-candle")]
pub use candle_training::{CandleTrainer, CandleTrainerConfig};
pub use error::{AiError, AiResult, LimitKind};
pub use execution_queue::{ExecutionQueueConfig, ExecutionQueueSnapshot};
pub use generation::{
    AiGenerationOptions, AiMessage, AiMessageRole, AiStructuredOutput, AiStructuredOutputFallback,
    AiToolCall, AiToolChoice, AiToolDefinition,
};
pub use governor::ResourceGovernor;
pub use hardware_sampler::{HardwareSampler, HardwareSamplerMetrics, SystemHardwareProbe};
pub use id::{BackendId, CapabilityId, DeviceId, ModelId, PeerId};
pub use lightweight::{
    LightweightCertainty, LightweightEngine, LightweightOutcome, LightweightReason,
    LightweightResolver, RuleMatch, TextRule,
};
#[cfg(feature = "backend-candle")]
pub use linear_format::NativeLinearArtifact;
pub use modality::AiModality;
pub use model::{
    ArtifactFormat, ArtifactIdentity, ArtifactLocation, ModelDescriptor, ModelRecord,
    ModelRegistry, ModelRegistrySnapshot, ModelState, QualityTier, Quantization,
};
pub use model_load::ModelLoadSnapshot;
pub use observability::{
    AiObservation, AiObservationSink, AiPlacementClass, AiTaskClass, AiTelemetry,
    AiTelemetrySnapshot, IgnoreAiObservations,
};
#[cfg(feature = "backend-openai-compatible")]
pub use openai_backend::OpenAiCompatibleBackend;
#[cfg(feature = "backend-openai-compatible")]
pub use openai_config::{
    OpenAiCompatibilityProfile, OpenAiCompatibleConfig, OpenAiCompatibleEngine,
    OpenAiExtraParameter, OpenAiGenerationCapabilities, OpenAiTokenLimitField,
};
#[cfg(feature = "backend-openai-compatible")]
pub use openai_transport::{
    OpenAiCompatibleTransport, OpenAiTransportChunkSink, OpenAiTransportFuture,
    OpenAiTransportRequest, OpenAiTransportResponse, UnauthenticatedOpenAiHttpTransport,
};
pub use policy::{
    AiDistributionPolicy, AiExecutionMode, AiLatencyClass, AiOptions, AiPriority, AiPrivacyMode,
    AiQualityTarget, AiResourceLimits, AiResourceMode,
};
pub use queue::{
    FairQueue, FairQueueConfig, FairQueueMetrics, QueueAdmission, QueueRejectionReason, QueuedWork,
};
pub use request::{AiContent, AiInput, AiLimits, AiRequest, AiTask};
pub use residency::{
    ResidencyConfig, ResidencyDecision, ResidencyEviction, ResidencyPlanner, ResidencyRecord,
    ResidencyRequest, ResidencyReservation, ResidencyTier, TierCapacity,
};
pub use residency_metrics::ResidencyMetrics;
pub use resource::{
    AcceleratorProbe, AcceleratorSample, AdmissionDecision, AdmissionReason, AiContributionPolicy,
    DeviceApi, DeviceCapabilities, DeviceClass, DeviceKind, DeviceMemoryKind, DeviceSnapshot,
    HardwareProbe, ResourceBudget, ResourceBudgetPair, ResourceEstimate, ResourceEstimateBreakdown,
    ResourceGovernorConfig, ResourceGovernorMetrics, ResourceProbeComponent, ResourceProbeFailure,
    ResourceProbeFailureKind, ResourceProbeStatus, ResourceSnapshot, ThermalPressure,
};
pub use response::{
    AiMetadata, AiOutput, AiResponse, AiScore, ExecutionAttempt, ExecutionDecision,
    ExecutionTarget, RouteReason,
};
pub use router::AiRuntime;
pub use runtime_health::AiRuntimeHealth;
pub use scheduler::{
    ComputeTarget, CostScheduler, PlacementCandidate, PlacementContext, PlacementKey,
    PlacementMetrics, PlacementPlan, PlacementPlanner, PlacementRejection,
    PlacementRejectionReason, SchedulerWeights, ScoredPlacement,
};
pub use security::{
    AiAuthorizationContext, AiSecretReference, ArtifactProvenance, ArtifactProvenanceVerifier,
    ModelSecurityPolicy, ProvenanceArtifactStore, REMOTE_COMPUTE_GRANT, REMOTE_STORAGE_GRANT,
};
pub use streaming::{AiStreamEvent, AiStreamSink};
#[cfg(feature = "swarm")]
pub use swarm::{
    AdvertisedCompute, AdvertisedStorage, AiNodeCapabilities, PeerAuthorization,
    PeerCapabilityAuthorizer, PeerCapabilityDirectory, PeerDirectoryConfig, PeerDirectoryMetrics,
    SwarmBridge, SwarmRoute,
};
#[cfg(feature = "training-candle")]
pub use training::{
    GovernorTrainingAdmission, IgnoreTrainingProgress, InMemoryTrainingDataset, TrainingAdmission,
    TrainingBackend, TrainingCheckpointPolicy, TrainingDataset, TrainingExample, TrainingFuture,
    TrainingJob, TrainingOutput, TrainingProgress, TrainingProgressObserver,
};

#[cfg(test)]
mod tests;
