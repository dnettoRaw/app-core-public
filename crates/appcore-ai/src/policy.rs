// =============================================================================
//        #######
//     ###       ###     F: policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{
    AiAuthorizationContext, AiGenerationOptions, AiSecretReference, BackendId, DeviceId, ModelId,
    QualityTier,
};
use std::time::Duration;

/// Requested scheduling priority.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub enum AiPriority {
    /// Work that may yield to all other classes.
    Background,
    /// Normal interactive or batch work.
    #[default]
    Normal,
    /// Important work admitted ahead of normal work without bypassing limits.
    High,
    /// Urgent work that still remains bounded and subject to authorization.
    Critical,
}

/// Latency and throughput preference.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AiLatencyClass {
    /// Prefer minimum response latency.
    Interactive,
    /// Balance response latency and throughput.
    #[default]
    Balanced,
    /// Prefer aggregate throughput within the deadline.
    Throughput,
    /// Work without an interactive latency target.
    Background,
}

/// Minimum answer-quality profile requested from model routing.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AiQualityTarget {
    /// Admit the smallest capable model and minimize latency and resource cost.
    #[default]
    Fast,
    /// Require at least a small model while preserving interactive operation.
    Balanced,
    /// Require a balanced or larger model for more demanding analysis.
    Deep,
    /// Require a model explicitly classified in the largest quality tier.
    Maximum,
}

impl AiQualityTarget {
    /// Returns the minimum model quality tier admitted by this profile.
    #[must_use]
    pub fn minimum_tier(self) -> QualityTier {
        match self {
            Self::Fast => QualityTier::Tiny,
            Self::Balanced => QualityTier::Small,
            Self::Deep => QualityTier::Balanced,
            Self::Maximum => QualityTier::Large,
        }
    }
}

/// Policy controlling where request content may be processed.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AiPrivacyMode {
    /// Content and artifacts must remain on the local node.
    LocalOnly,
    /// Authenticated AppCore peers in the same authorized scope may process it.
    TrustedSwarm,
    /// Explicitly configured remote providers may process it.
    #[default]
    RemoteAllowed,
}

/// Runtime placement mode.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AiExecutionMode {
    /// Use only local compute and policy-permitted local artifact storage.
    Local,
    /// Require an authorized swarm route.
    Swarm,
    /// Compare policy-permitted local and swarm routes.
    #[default]
    Auto,
}

/// Explicit resource caps for custom resource mode.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiResourceLimits {
    /// Maximum CPU utilization percentage from 1 through 100.
    pub max_cpu_percent: u8,
    /// Maximum RAM committed to AI work.
    pub max_memory_bytes: u64,
    /// Maximum VRAM committed to AI work.
    pub max_vram_bytes: u64,
    /// Maximum workers.
    pub max_workers: usize,
    /// Maximum concurrent jobs.
    pub max_concurrent_jobs: usize,
}

/// Resource-use profile applied by the governor.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AiResourceMode {
    /// Preserve substantial host headroom.
    Eco,
    /// Preserve normal interactive host headroom.
    #[default]
    Balanced,
    /// Prefer AI throughput while retaining a small safety headroom.
    Performance,
    /// Remove voluntary AppCore headroom without changing hardware safeguards.
    Unrestricted,
    /// Apply explicit validated limits.
    Custom(AiResourceLimits),
}

/// Policy for consuming distributed compute and artifact locations.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AiDistributionPolicy {
    /// Whether an authenticated remote node may execute the request.
    pub allow_remote_compute: bool,
    /// Whether artifacts may be fetched from authenticated peer storage.
    pub allow_remote_storage: bool,
    /// Whether equal-cost routes prefer the local node.
    pub prefer_local: bool,
    /// Maximum peers considered by one plan.
    pub max_peers: usize,
    /// Maximum admitted network latency to a remote compute target.
    pub max_remote_latency: Duration,
}

impl Default for AiDistributionPolicy {
    fn default() -> Self {
        Self {
            allow_remote_compute: false,
            allow_remote_storage: false,
            prefer_local: true,
            max_peers: 8,
            max_remote_latency: Duration::from_millis(250),
        }
    }
}

/// Backend-neutral overrides and policies for one request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiOptions {
    /// Scheduling priority.
    pub priority: AiPriority,
    /// Latency preference.
    pub latency: AiLatencyClass,
    /// Minimum answer-quality profile.
    pub quality: AiQualityTarget,
    /// Content placement policy.
    pub privacy: AiPrivacyMode,
    /// Local resource profile.
    pub resources: AiResourceMode,
    /// Local, swarm or automatic placement.
    pub execution: AiExecutionMode,
    /// Distributed route constraints.
    pub distribution: AiDistributionPolicy,
    /// Authenticated tenant and grants required for every remote route.
    pub authorization: Option<AiAuthorizationContext>,
    /// Optional unresolved provider credential reference; never a secret value.
    pub credential: Option<AiSecretReference>,
    /// Optional required model.
    pub model: Option<ModelId>,
    /// Optional required backend.
    pub backend: Option<BackendId>,
    /// Optional required device.
    pub device: Option<DeviceId>,
    /// Whether bounded escalation may try another route.
    pub allow_escalation: bool,
    /// Whether a safe structured execution decision is returned.
    pub include_diagnostics: bool,
    /// Relative deadline from admission to completion.
    pub deadline: Option<Duration>,
    /// Maximum backend-neutral cost units accepted by the caller.
    pub max_cost_units: Option<u64>,
    /// Bounded controls used by generative backends.
    pub generation: AiGenerationOptions,
}

impl Default for AiOptions {
    fn default() -> Self {
        Self {
            priority: AiPriority::default(),
            latency: AiLatencyClass::default(),
            quality: AiQualityTarget::default(),
            privacy: AiPrivacyMode::default(),
            resources: AiResourceMode::default(),
            execution: AiExecutionMode::default(),
            distribution: AiDistributionPolicy::default(),
            authorization: None,
            credential: None,
            model: None,
            backend: None,
            device: None,
            allow_escalation: true,
            include_diagnostics: false,
            deadline: Some(Duration::from_secs(30)),
            max_cost_units: None,
            generation: AiGenerationOptions::default(),
        }
    }
}
