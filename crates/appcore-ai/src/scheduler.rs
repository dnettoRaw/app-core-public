// =============================================================================
//        #######
//     ###       ###     F: scheduler.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::scheduler_score::{reject, score};
use crate::{
    AiLatencyClass, AiPriority, AiResourceMode, ArtifactLocation, BackendHealth, BackendId,
    DeviceId, DeviceKind, ModelId, PeerId, ResourceEstimate,
};
use std::collections::BTreeMap;
use std::sync::RwLock;
use std::time::Duration;

const MAX_LEARNED_ROUTES: usize = 4_096;

/// A local or authenticated remote compute destination.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum ComputeTarget {
    /// Local general-purpose processor.
    LocalCpu(DeviceId),
    /// Local graphics processor.
    LocalGpu(DeviceId),
    /// Local neural accelerator.
    LocalNpu(DeviceId),
    /// Authenticated remote compute device.
    RemotePeer {
        /// Stable peer identity used internally, never in public diagnostics.
        peer: PeerId,
        /// Peer-owned device identity.
        device: DeviceId,
        /// Backend-neutral remote hardware class.
        kind: DeviceKind,
    },
}

impl ComputeTarget {
    /// Creates a local target from a backend device declaration.
    #[must_use]
    pub fn local(kind: DeviceKind, device: DeviceId) -> Self {
        match kind {
            DeviceKind::Cpu => Self::LocalCpu(device),
            DeviceKind::Gpu => Self::LocalGpu(device),
            DeviceKind::Npu => Self::LocalNpu(device),
        }
    }

    /// Returns whether this target belongs to another node.
    #[must_use]
    pub fn is_remote(&self) -> bool {
        matches!(self, Self::RemotePeer { .. })
    }
}

/// Stable identity of one placement route.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct PlacementKey {
    /// Logical model identity.
    pub model: ModelId,
    /// Backend identity.
    pub backend: BackendId,
    /// Compute destination.
    pub target: ComputeTarget,
}

/// Recent bounded observations supplied by a backend or swarm bridge.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PlacementMetrics {
    /// Current device utilization percentage when known.
    pub load_percent: Option<u8>,
    /// Current bounded queue depth.
    pub queue_depth: usize,
    /// Available system memory relevant to this target.
    pub available_memory_bytes: Option<u64>,
    /// Available device-local memory when applicable.
    pub available_vram_bytes: Option<u64>,
    /// Recent end-to-end latency exponential moving average.
    pub latency_ema_ms: Option<u64>,
    /// Recent throughput exponential moving average in items per second.
    pub throughput_ema: Option<u64>,
}

/// One scheduler candidate with compute and artifact costs kept separate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementCandidate {
    /// Stable placement identity.
    pub key: PlacementKey,
    /// Backend health.
    pub health: BackendHealth,
    /// Estimated peak resources.
    pub resources: ResourceEstimate,
    /// Latest bounded metrics.
    pub metrics: PlacementMetrics,
    /// Whether the model is already resident at the compute destination.
    pub model_resident: bool,
    /// Chosen artifact source, independent from compute placement.
    pub artifact_source: Option<ArtifactLocation>,
    /// Estimated model activation time.
    pub load_time_ms: u64,
    /// Estimated artifact transfer cost.
    pub transfer_cost_units: u64,
    /// Estimated inference cost.
    pub inference_cost_units: u64,
    /// Remote round-trip latency, if applicable.
    pub rtt_ms: Option<u64>,
    /// Remote available bandwidth in bytes per second, if known.
    pub bandwidth_bytes_per_second: Option<u64>,
    /// Whether peer authentication, trust and request policy permit the target.
    pub trusted: bool,
    /// Relative cost of failing over after this route starts.
    pub failover_cost_units: u64,
}

/// Request-specific scheduler constraints.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlacementContext {
    /// Request priority.
    pub priority: AiPriority,
    /// Requested latency-versus-throughput profile.
    pub latency_class: AiLatencyClass,
    /// Resource profile.
    pub resource_mode: AiResourceMode,
    /// Remaining deadline from the injected clock.
    pub deadline_remaining: Option<Duration>,
    /// Whether policy permits remote compute.
    pub allow_remote: bool,
    /// Whether equal-cost routes should preserve local execution.
    pub prefer_local: bool,
    /// Maximum admitted remote RTT.
    pub max_remote_latency: Duration,
    /// Whether the resource governor currently reports host pressure.
    pub pressure_limited: bool,
}

/// Deterministic integer scheduler weights. Lower total score wins.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SchedulerWeights {
    /// Device load penalty.
    pub load: u64,
    /// Queue-depth penalty.
    pub queue: u64,
    /// Recent latency penalty.
    pub latency: u64,
    /// Cold activation penalty.
    pub cold_load: u64,
    /// Artifact transfer penalty.
    pub transfer: u64,
    /// Inference cost penalty.
    pub inference: u64,
    /// Remote RTT penalty.
    pub remote_latency: u64,
    /// Additional remote penalty when request policy prefers local execution.
    pub local_preference: u64,
    /// Failover penalty.
    pub failover: u64,
    /// Model-residency reward.
    pub residency_reward: u64,
    /// Throughput reward.
    pub throughput_reward: u64,
    /// Degraded-health penalty.
    pub degraded: u64,
}

impl Default for SchedulerWeights {
    fn default() -> Self {
        Self {
            load: 4,
            queue: 25,
            latency: 2,
            cold_load: 3,
            transfer: 2,
            inference: 5,
            remote_latency: 3,
            local_preference: 500,
            failover: 2,
            residency_reward: 1_000,
            throughput_reward: 1,
            degraded: 2_000,
        }
    }
}

/// Structured reason a candidate cannot enter the ranked plan.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlacementRejectionReason {
    /// Backend or device is unavailable.
    Unavailable,
    /// Remote trust or request policy denied the target.
    Policy,
    /// Known RAM capacity is insufficient.
    Memory,
    /// Known VRAM capacity is insufficient.
    Vram,
    /// Expected work cannot meet the request deadline.
    Deadline,
    /// Conservative mode rejected a heavily loaded target under pressure.
    Pressure,
}

/// One excluded scheduler candidate.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementRejection {
    /// Excluded route.
    pub key: PlacementKey,
    /// Stable exclusion reason.
    pub reason: PlacementRejectionReason,
}

/// One admitted route and its explainable deterministic score.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScoredPlacement {
    /// Admitted route.
    pub key: PlacementKey,
    /// Lower-is-better saturating integer score.
    pub score: u64,
    /// Artifact source selected separately from compute.
    pub artifact_source: Option<ArtifactLocation>,
}

/// Complete bounded result of one placement pass.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlacementPlan {
    /// Admitted routes in deterministic score and identity order.
    pub ordered: Vec<ScoredPlacement>,
    /// Explicitly excluded routes.
    pub rejected: Vec<PlacementRejection>,
}

/// Backend-neutral placement boundary consumed by `AiRuntime`.
pub trait PlacementPlanner: Send + Sync {
    /// Ranks compatible routes without performing I/O.
    fn plan(&self, context: PlacementContext, candidates: &[PlacementCandidate]) -> PlacementPlan;
}

/// Cost planner with deterministic integer exponential moving averages.
#[derive(Debug)]
pub struct CostScheduler {
    weights: SchedulerWeights,
    ema_alpha_basis_points: u16,
    learned: RwLock<BTreeMap<PlacementKey, PlacementMetrics>>,
}

impl CostScheduler {
    /// Creates a planner and validates its fixed-point EMA alpha.
    pub fn new(weights: SchedulerWeights, ema_alpha_basis_points: u16) -> crate::AiResult<Self> {
        if ema_alpha_basis_points == 0 || ema_alpha_basis_points > 10_000 {
            return Err(crate::AiError::InvalidInput("scheduler EMA alpha"));
        }
        Ok(Self {
            weights,
            ema_alpha_basis_points,
            learned: RwLock::new(BTreeMap::new()),
        })
    }

    /// Records one recent sample using deterministic fixed-point EMA updates.
    pub fn observe(&self, key: PlacementKey, sample: PlacementMetrics) -> crate::AiResult<()> {
        let mut learned = self
            .learned
            .write()
            .map_err(|_| crate::AiError::InternalState)?;
        if !learned.contains_key(&key) && learned.len() >= MAX_LEARNED_ROUTES {
            return Err(crate::AiError::Capacity("scheduler learned routes"));
        }
        let previous = learned.get(&key).copied().unwrap_or_default();
        learned.insert(
            key,
            merge_metrics(previous, sample, self.ema_alpha_basis_points),
        );
        Ok(())
    }
}

impl Default for CostScheduler {
    fn default() -> Self {
        Self {
            weights: SchedulerWeights::default(),
            ema_alpha_basis_points: 2_000,
            learned: RwLock::new(BTreeMap::new()),
        }
    }
}

impl PlacementPlanner for CostScheduler {
    fn plan(&self, context: PlacementContext, candidates: &[PlacementCandidate]) -> PlacementPlan {
        let learned = self.learned.read().ok();
        let mut ordered = Vec::new();
        let mut rejected = Vec::new();
        for candidate in candidates {
            let metrics = learned
                .as_ref()
                .and_then(|values| values.get(&candidate.key))
                .copied()
                .map_or(candidate.metrics, |value| overlay(candidate.metrics, value));
            match reject(context, candidate, metrics) {
                Some(reason) => rejected.push(PlacementRejection {
                    key: candidate.key.clone(),
                    reason,
                }),
                None => ordered.push(ScoredPlacement {
                    key: candidate.key.clone(),
                    score: score(self.weights, context, candidate, metrics),
                    artifact_source: candidate.artifact_source.clone(),
                }),
            }
        }
        ordered.sort_by(|left, right| left.score.cmp(&right.score).then(left.key.cmp(&right.key)));
        rejected.sort_by(|left, right| left.key.cmp(&right.key));
        PlacementPlan { ordered, rejected }
    }
}

fn merge_metrics(
    previous: PlacementMetrics,
    sample: PlacementMetrics,
    alpha: u16,
) -> PlacementMetrics {
    PlacementMetrics {
        load_percent: sample.load_percent.or(previous.load_percent),
        queue_depth: sample.queue_depth,
        available_memory_bytes: sample
            .available_memory_bytes
            .or(previous.available_memory_bytes),
        available_vram_bytes: sample
            .available_vram_bytes
            .or(previous.available_vram_bytes),
        latency_ema_ms: ema(previous.latency_ema_ms, sample.latency_ema_ms, alpha),
        throughput_ema: ema(previous.throughput_ema, sample.throughput_ema, alpha),
    }
}

fn overlay(current: PlacementMetrics, learned: PlacementMetrics) -> PlacementMetrics {
    PlacementMetrics {
        load_percent: current.load_percent.or(learned.load_percent),
        queue_depth: current.queue_depth,
        available_memory_bytes: current
            .available_memory_bytes
            .or(learned.available_memory_bytes),
        available_vram_bytes: current
            .available_vram_bytes
            .or(learned.available_vram_bytes),
        latency_ema_ms: learned.latency_ema_ms.or(current.latency_ema_ms),
        throughput_ema: learned.throughput_ema.or(current.throughput_ema),
    }
}

fn ema(previous: Option<u64>, sample: Option<u64>, alpha: u16) -> Option<u64> {
    match (previous, sample) {
        (Some(previous), Some(sample)) => Some(
            previous
                .saturating_mul(u64::from(10_000 - alpha))
                .saturating_add(sample.saturating_mul(u64::from(alpha)))
                / 10_000,
        ),
        (_, Some(sample)) => Some(sample),
        (previous, None) => previous,
    }
}
