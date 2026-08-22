// =============================================================================
//        #######
//     ###       ###     F: observability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::{AiExecutionMode, AiTask, DeviceKind, ExecutionTarget};
use std::fmt::{Debug, Formatter};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

const LATENCY_BOUNDS_MS: [u64; 11] = [1, 5, 10, 25, 50, 100, 250, 500, 1_000, 5_000, u64::MAX];

/// Low-cardinality task class for metrics and tracing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiTaskClass {
    /// Generation or transformation.
    Text,
    /// Classification, extraction or decision.
    Decision,
    /// Embedding.
    Embedding,
    /// Image-understanding work.
    Image,
    /// Document-understanding work.
    Document,
    /// Consumer-owned capability.
    Custom,
}

impl From<&AiTask> for AiTaskClass {
    fn from(task: &AiTask) -> Self {
        match task {
            AiTask::GenerateText | AiTask::Chat | AiTask::TransformText => Self::Text,
            AiTask::ClassifyText | AiTask::Extract | AiTask::Decide => Self::Decision,
            AiTask::Embed => Self::Embedding,
            AiTask::AnalyzeImage => Self::Image,
            AiTask::AnalyzeDocument => Self::Document,
            AiTask::Capability(_) => Self::Custom,
        }
    }
}

/// Low-cardinality execution placement.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AiPlacementClass {
    /// Deterministic in-process resolver.
    Lightweight,
    /// Local backend.
    Local,
    /// Authenticated remote backend.
    Remote,
}

/// Redacted structured event suitable for an `appcore-ops` adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiObservation {
    /// One request entered validation and resolution.
    RequestStarted {
        /// Bounded task class.
        task: AiTaskClass,
        /// Requested local/swarm/auto mode.
        mode: AiExecutionMode,
    },
    /// A route was selected without model, backend, device or peer IDs.
    RouteSelected {
        /// Placement class.
        placement: AiPlacementClass,
        /// Hardware class when applicable.
        device: Option<DeviceKind>,
        /// Whether a prior attempt failed.
        escalated: bool,
    },
    /// Resource admission rejected or deferred a candidate.
    AdmissionRestricted,
    /// One model load completed.
    ModelLoad {
        /// Load latency.
        latency: Duration,
        /// Whether activation succeeded.
        success: bool,
    },
    /// One request completed.
    RequestCompleted {
        /// Whether resolution succeeded.
        success: bool,
        /// End-to-end latency.
        latency: Duration,
        /// Bounded attempted routes.
        attempts: usize,
    },
}

/// Sink boundary implemented by the AppCore composition root with `appcore-ops`.
pub trait AiObservationSink: Send + Sync {
    /// Records one payload-free observation.
    fn record(&self, observation: &AiObservation);
}

/// No-op sink used by the default independent composition.
#[derive(Clone, Copy, Debug, Default)]
pub struct IgnoreAiObservations;

impl AiObservationSink for IgnoreAiObservations {
    fn record(&self, _observation: &AiObservation) {}
}

/// Stable aggregate metrics with fixed latency buckets and no arbitrary-ID labels.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AiTelemetrySnapshot {
    /// Requests submitted to validation and resolution.
    pub requests: u64,
    /// Successful requests.
    pub successes: u64,
    /// Failed requests.
    pub failures: u64,
    /// Admission rejections or deferrals.
    pub admission_restricted: u64,
    /// Requests with more than one execution attempt.
    pub escalations: u64,
    /// Lightweight answers retained as bounded fallback before backend escalation.
    pub fallbacks: u64,
    /// Successful model activations.
    pub model_load_successes: u64,
    /// Failed model activations.
    pub model_load_failures: u64,
    /// Lightweight placements.
    pub lightweight_placements: u64,
    /// Local backend placements.
    pub local_placements: u64,
    /// Remote placements.
    pub remote_placements: u64,
    /// Remote failovers inferred from multiple attempts.
    pub remote_failovers: u64,
    /// Approximate p50 upper latency bound.
    pub latency_p50: Duration,
    /// Approximate p95 upper latency bound.
    pub latency_p95: Duration,
    /// Approximate p99 upper latency bound.
    pub latency_p99: Duration,
}

/// Thread-safe telemetry recorder owned by one `AiRuntime`.
pub struct AiTelemetry {
    sink: Arc<dyn AiObservationSink>,
    requests: AtomicU64,
    successes: AtomicU64,
    failures: AtomicU64,
    admission_restricted: AtomicU64,
    escalations: AtomicU64,
    fallbacks: AtomicU64,
    model_load_successes: AtomicU64,
    model_load_failures: AtomicU64,
    lightweight: AtomicU64,
    local: AtomicU64,
    remote: AtomicU64,
    remote_failovers: AtomicU64,
    latency_buckets: [AtomicU64; LATENCY_BOUNDS_MS.len()],
}

impl Debug for AiTelemetry {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AiTelemetry")
            .field("snapshot", &self.snapshot())
            .finish_non_exhaustive()
    }
}

impl AiTelemetry {
    /// Creates zeroed telemetry connected to one sink.
    #[must_use]
    pub fn new(sink: Arc<dyn AiObservationSink>) -> Self {
        Self {
            sink,
            requests: AtomicU64::new(0),
            successes: AtomicU64::new(0),
            failures: AtomicU64::new(0),
            admission_restricted: AtomicU64::new(0),
            escalations: AtomicU64::new(0),
            fallbacks: AtomicU64::new(0),
            model_load_successes: AtomicU64::new(0),
            model_load_failures: AtomicU64::new(0),
            lightweight: AtomicU64::new(0),
            local: AtomicU64::new(0),
            remote: AtomicU64::new(0),
            remote_failovers: AtomicU64::new(0),
            latency_buckets: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    pub(crate) fn request_started(&self, task: &AiTask, mode: AiExecutionMode) {
        self.requests.fetch_add(1, Ordering::Relaxed);
        self.sink.record(&AiObservation::RequestStarted {
            task: task.into(),
            mode,
        });
    }

    pub(crate) fn admission_restricted(&self) {
        self.admission_restricted.fetch_add(1, Ordering::Relaxed);
        self.sink.record(&AiObservation::AdmissionRestricted);
    }

    pub(crate) fn fallback(&self) {
        self.fallbacks.fetch_add(1, Ordering::Relaxed);
    }

    pub(crate) fn model_load(&self, latency: Duration, success: bool) {
        if success {
            self.model_load_successes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.model_load_failures.fetch_add(1, Ordering::Relaxed);
        }
        self.sink
            .record(&AiObservation::ModelLoad { latency, success });
    }

    pub(crate) fn route_selected(
        &self,
        target: &ExecutionTarget,
        device: Option<DeviceKind>,
        escalated: bool,
    ) {
        let placement = match target {
            ExecutionTarget::Lightweight => {
                self.lightweight.fetch_add(1, Ordering::Relaxed);
                AiPlacementClass::Lightweight
            }
            ExecutionTarget::Local { .. } => {
                self.local.fetch_add(1, Ordering::Relaxed);
                AiPlacementClass::Local
            }
            ExecutionTarget::Remote { .. } => {
                self.remote.fetch_add(1, Ordering::Relaxed);
                if escalated {
                    self.remote_failovers.fetch_add(1, Ordering::Relaxed);
                }
                AiPlacementClass::Remote
            }
        };
        if escalated {
            self.escalations.fetch_add(1, Ordering::Relaxed);
        }
        self.sink.record(&AiObservation::RouteSelected {
            placement,
            device,
            escalated,
        });
    }

    pub(crate) fn completed(&self, success: bool, latency: Duration, attempts: usize) {
        if success {
            self.successes.fetch_add(1, Ordering::Relaxed);
        } else {
            self.failures.fetch_add(1, Ordering::Relaxed);
        }
        let latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
        let index = LATENCY_BOUNDS_MS
            .iter()
            .position(|bound| latency_ms <= *bound)
            .unwrap_or(LATENCY_BOUNDS_MS.len() - 1);
        self.latency_buckets[index].fetch_add(1, Ordering::Relaxed);
        self.sink.record(&AiObservation::RequestCompleted {
            success,
            latency,
            attempts,
        });
    }

    /// Returns an internally consistent-enough lock-free metrics snapshot.
    #[must_use]
    pub fn snapshot(&self) -> AiTelemetrySnapshot {
        let buckets = self
            .latency_buckets
            .each_ref()
            .map(|value| value.load(Ordering::Relaxed));
        let total = buckets.iter().copied().sum();
        AiTelemetrySnapshot {
            requests: self.requests.load(Ordering::Relaxed),
            successes: self.successes.load(Ordering::Relaxed),
            failures: self.failures.load(Ordering::Relaxed),
            admission_restricted: self.admission_restricted.load(Ordering::Relaxed),
            escalations: self.escalations.load(Ordering::Relaxed),
            fallbacks: self.fallbacks.load(Ordering::Relaxed),
            model_load_successes: self.model_load_successes.load(Ordering::Relaxed),
            model_load_failures: self.model_load_failures.load(Ordering::Relaxed),
            lightweight_placements: self.lightweight.load(Ordering::Relaxed),
            local_placements: self.local.load(Ordering::Relaxed),
            remote_placements: self.remote.load(Ordering::Relaxed),
            remote_failovers: self.remote_failovers.load(Ordering::Relaxed),
            latency_p50: percentile(&buckets, total, 50),
            latency_p95: percentile(&buckets, total, 95),
            latency_p99: percentile(&buckets, total, 99),
        }
    }
}

impl Default for AiTelemetry {
    fn default() -> Self {
        Self::new(Arc::new(IgnoreAiObservations))
    }
}

fn percentile(buckets: &[u64; LATENCY_BOUNDS_MS.len()], total: u64, percent: u64) -> Duration {
    if total == 0 {
        return Duration::ZERO;
    }
    let target = total.saturating_mul(percent).div_ceil(100);
    let mut cumulative = 0u64;
    for (index, count) in buckets.iter().enumerate() {
        cumulative = cumulative.saturating_add(*count);
        if cumulative >= target {
            return Duration::from_millis(LATENCY_BOUNDS_MS[index]);
        }
    }
    Duration::from_millis(u64::MAX)
}
