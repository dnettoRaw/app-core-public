// =============================================================================
//        #######
//     ###       ###     F: telemetry.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounded vendor-neutral Gateway telemetry contracts.

use parking_lot::Mutex;
use serde::Serialize;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// Maximum distinct capability series retained by one Gateway process.
pub const MAX_GATEWAY_TELEMETRY_CAPABILITIES: usize = 128;

const OVERFLOW_CAPABILITY: &str = "appcore.gateway.capability.overflow";
const LATENCY_BUCKETS_NS: [u64; 12] = [
    100_000,
    500_000,
    1_000_000,
    5_000_000,
    10_000_000,
    50_000_000,
    100_000_000,
    500_000_000,
    1_000_000_000,
    5_000_000_000,
    30_000_000_000,
    u64::MAX,
];
const PAYLOAD_BUCKETS_BYTES: [u64; 8] = [
    1_024,
    4_096,
    16_384,
    65_536,
    262_144,
    1_048_576,
    4_194_304,
    u64::MAX,
];

/// Immutable telemetry for one bounded capability label.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GatewayCapabilityTelemetrySnapshot {
    /// Validated capability name or the fixed overflow label.
    pub capability: String,
    /// Total observed routes.
    pub requests: u64,
    /// Routes completed with a worker response.
    pub successes: u64,
    /// Invalid protocol or payload routes.
    pub invalid_requests: u64,
    /// Routes rejected because no compatible worker was available.
    pub worker_unavailable: u64,
    /// Routes rejected because the explicit target was stale or disconnected.
    pub worker_unhealthy: u64,
    /// Routes rejected by the selected worker's in-flight bound.
    pub worker_at_capacity: u64,
    /// Routes rejected by the bounded pending-request limit.
    pub pending_saturation: u64,
    /// Routes rejected by a full worker outbound queue.
    pub queue_saturation: u64,
    /// Routes that lost the selected worker or transport.
    pub transport_failures: u64,
    /// Routes that exceeded their bounded response deadline.
    pub timeouts: u64,
    /// Routes cancelled by Gateway shutdown.
    pub shutdowns: u64,
    /// Fixed-bucket p50 upper bound for complete route latency.
    pub latency_p50_ns: u64,
    /// Fixed-bucket p95 upper bound for complete route latency.
    pub latency_p95_ns: u64,
    /// Fixed-bucket p99 upper bound for complete route latency.
    pub latency_p99_ns: u64,
    /// Fixed-bucket p50 upper bound after dispatch to the worker.
    pub worker_wait_p50_ns: u64,
    /// Fixed-bucket p95 upper bound after dispatch to the worker.
    pub worker_wait_p95_ns: u64,
    /// Fixed-bucket p99 upper bound after dispatch to the worker.
    pub worker_wait_p99_ns: u64,
    /// Fixed-bucket p50 upper bound for tenant-lock acquisition.
    pub lock_wait_p50_ns: u64,
    /// Fixed-bucket p95 upper bound for tenant-lock acquisition.
    pub lock_wait_p95_ns: u64,
    /// Fixed-bucket p99 upper bound for tenant-lock acquisition.
    pub lock_wait_p99_ns: u64,
    /// Fixed-bucket p50 upper bound for opaque payload bytes.
    pub payload_p50_bytes: u64,
    /// Fixed-bucket p95 upper bound for opaque payload bytes.
    pub payload_p95_bytes: u64,
    /// Fixed-bucket p99 upper bound for opaque payload bytes.
    pub payload_p99_bytes: u64,
}

/// Immutable process-wide Gateway telemetry snapshot.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct GatewayTelemetrySnapshot {
    /// Routes currently admitted and not yet completed.
    pub inflight: u64,
    /// Highest observed concurrent route count.
    pub inflight_peak: u64,
    /// Highest observed worker outbound queue depth.
    pub queue_depth_peak: u64,
    /// Highest observed admitted request count on one worker.
    pub worker_inflight_peak: u64,
    /// Worker connections that replaced an existing logical worker.
    pub reconnects: u64,
    /// Explicit route retry attempts; zero means no retry was performed.
    pub retries: u64,
    /// Routes rejected by pending or outbound queue saturation.
    pub saturations: u64,
    /// Routes that exceeded their bounded worker-response deadline.
    pub timeouts: u64,
    /// Explicit targets rejected as stale or disconnected.
    pub worker_unhealthy_rejections: u64,
    /// Explicit targets rejected by the per-worker in-flight bound.
    pub worker_capacity_rejections: u64,
    /// Authentication failures without credential or identity labels.
    pub authentication_failures: u64,
    /// Capability samples aggregated into the fixed overflow series.
    pub capability_overflow: u64,
    /// Failed calls to an explicit telemetry exporter.
    pub export_failures: u64,
    /// Deterministically ordered bounded capability series.
    pub capabilities: Vec<GatewayCapabilityTelemetrySnapshot>,
}

/// Controlled failure returned by a deployment-owned telemetry exporter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("gateway telemetry export failed")]
pub struct GatewayTelemetryExportError;

/// Vendor-neutral adapter invoked explicitly with an owned bounded snapshot.
pub trait GatewayTelemetryExporter: Send + Sync {
    /// Exports one immutable snapshot without access to routing state.
    fn export(
        &self,
        snapshot: &GatewayTelemetrySnapshot,
    ) -> Result<(), GatewayTelemetryExportError>;
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum RouteOutcome {
    Success,
    Invalid,
    WorkerUnavailable,
    WorkerUnhealthy,
    WorkerAtCapacity,
    PendingSaturation,
    QueueSaturation,
    TransportFailure,
    Timeout,
    Shutdown,
}

pub(crate) struct RouteSample {
    pub(crate) capability: String,
    pub(crate) outcome: RouteOutcome,
    pub(crate) latency_ns: u64,
    pub(crate) worker_wait_ns: Option<u64>,
    pub(crate) lock_wait_ns: u64,
    pub(crate) payload_bytes: u64,
}

#[derive(Debug, Default)]
pub(crate) struct GatewayTelemetryRegistry {
    inflight: AtomicU64,
    inflight_peak: AtomicU64,
    queue_depth_peak: AtomicU64,
    worker_inflight_peak: AtomicU64,
    reconnects: AtomicU64,
    retries: AtomicU64,
    saturations: AtomicU64,
    timeouts: AtomicU64,
    worker_unhealthy_rejections: AtomicU64,
    worker_capacity_rejections: AtomicU64,
    authentication_failures: AtomicU64,
    capability_overflow: AtomicU64,
    export_failures: AtomicU64,
    capabilities: Mutex<BTreeMap<String, CapabilitySeries>>,
}

impl GatewayTelemetryRegistry {
    pub(crate) fn route_started(&self) {
        let inflight = saturating_increment(&self.inflight);
        update_max(&self.inflight_peak, inflight);
    }

    pub(crate) fn route_finished(&self, sample: RouteSample) {
        saturating_decrement(&self.inflight);
        match sample.outcome {
            RouteOutcome::PendingSaturation | RouteOutcome::QueueSaturation => {
                saturating_increment(&self.saturations);
            }
            RouteOutcome::Timeout => {
                saturating_increment(&self.timeouts);
            }
            RouteOutcome::WorkerUnhealthy => {
                saturating_increment(&self.worker_unhealthy_rejections);
            }
            RouteOutcome::WorkerAtCapacity => {
                saturating_increment(&self.worker_capacity_rejections);
                saturating_increment(&self.saturations);
            }
            _ => {}
        }
        let mut capabilities = self.capabilities.lock();
        let label = if capabilities.contains_key(&sample.capability)
            || capabilities.len() < MAX_GATEWAY_TELEMETRY_CAPABILITIES
        {
            sample.capability.as_str()
        } else {
            saturating_increment(&self.capability_overflow);
            OVERFLOW_CAPABILITY
        };
        capabilities
            .entry(label.to_string())
            .or_default()
            .record(&sample);
    }

    pub(crate) fn observe_queue_depth(&self, depth: usize) {
        update_max(
            &self.queue_depth_peak,
            u64::try_from(depth).unwrap_or(u64::MAX),
        );
    }

    pub(crate) fn observe_worker_inflight(&self, inflight: u64) {
        update_max(&self.worker_inflight_peak, inflight);
    }

    pub(crate) fn reconnect(&self) {
        saturating_increment(&self.reconnects);
    }

    pub(crate) fn retry(&self) {
        saturating_increment(&self.retries);
    }

    pub(crate) fn authentication_failure(&self) {
        saturating_increment(&self.authentication_failures);
    }

    pub(crate) fn export_failure(&self) {
        saturating_increment(&self.export_failures);
    }

    pub(crate) fn snapshot(&self) -> GatewayTelemetrySnapshot {
        let capabilities = self
            .capabilities
            .lock()
            .iter()
            .map(|(capability, series)| series.snapshot(capability.clone()))
            .collect();
        GatewayTelemetrySnapshot {
            inflight: self.inflight.load(Ordering::Relaxed),
            inflight_peak: self.inflight_peak.load(Ordering::Relaxed),
            queue_depth_peak: self.queue_depth_peak.load(Ordering::Relaxed),
            worker_inflight_peak: self.worker_inflight_peak.load(Ordering::Relaxed),
            reconnects: self.reconnects.load(Ordering::Relaxed),
            retries: self.retries.load(Ordering::Relaxed),
            saturations: self.saturations.load(Ordering::Relaxed),
            timeouts: self.timeouts.load(Ordering::Relaxed),
            worker_unhealthy_rejections: self.worker_unhealthy_rejections.load(Ordering::Relaxed),
            worker_capacity_rejections: self.worker_capacity_rejections.load(Ordering::Relaxed),
            authentication_failures: self.authentication_failures.load(Ordering::Relaxed),
            capability_overflow: self.capability_overflow.load(Ordering::Relaxed),
            export_failures: self.export_failures.load(Ordering::Relaxed),
            capabilities,
        }
    }
}

#[derive(Debug, Default)]
struct CapabilitySeries {
    requests: u64,
    successes: u64,
    invalid_requests: u64,
    worker_unavailable: u64,
    worker_unhealthy: u64,
    worker_at_capacity: u64,
    pending_saturation: u64,
    queue_saturation: u64,
    transport_failures: u64,
    timeouts: u64,
    shutdowns: u64,
    latency: FixedHistogram<12>,
    worker_wait: FixedHistogram<12>,
    lock_wait: FixedHistogram<12>,
    payload: FixedHistogram<8>,
}

impl CapabilitySeries {
    fn record(&mut self, sample: &RouteSample) {
        increment(&mut self.requests);
        match sample.outcome {
            RouteOutcome::Success => increment(&mut self.successes),
            RouteOutcome::Invalid => increment(&mut self.invalid_requests),
            RouteOutcome::WorkerUnavailable => increment(&mut self.worker_unavailable),
            RouteOutcome::WorkerUnhealthy => increment(&mut self.worker_unhealthy),
            RouteOutcome::WorkerAtCapacity => increment(&mut self.worker_at_capacity),
            RouteOutcome::PendingSaturation => increment(&mut self.pending_saturation),
            RouteOutcome::QueueSaturation => increment(&mut self.queue_saturation),
            RouteOutcome::TransportFailure => increment(&mut self.transport_failures),
            RouteOutcome::Timeout => increment(&mut self.timeouts),
            RouteOutcome::Shutdown => increment(&mut self.shutdowns),
        }
        self.latency.record(sample.latency_ns, &LATENCY_BUCKETS_NS);
        if let Some(worker_wait_ns) = sample.worker_wait_ns {
            self.worker_wait.record(worker_wait_ns, &LATENCY_BUCKETS_NS);
        }
        self.lock_wait
            .record(sample.lock_wait_ns, &LATENCY_BUCKETS_NS);
        self.payload
            .record(sample.payload_bytes, &PAYLOAD_BUCKETS_BYTES);
    }

    fn snapshot(&self, capability: String) -> GatewayCapabilityTelemetrySnapshot {
        GatewayCapabilityTelemetrySnapshot {
            capability,
            requests: self.requests,
            successes: self.successes,
            invalid_requests: self.invalid_requests,
            worker_unavailable: self.worker_unavailable,
            worker_unhealthy: self.worker_unhealthy,
            worker_at_capacity: self.worker_at_capacity,
            pending_saturation: self.pending_saturation,
            queue_saturation: self.queue_saturation,
            transport_failures: self.transport_failures,
            timeouts: self.timeouts,
            shutdowns: self.shutdowns,
            latency_p50_ns: self.latency.percentile(50, &LATENCY_BUCKETS_NS),
            latency_p95_ns: self.latency.percentile(95, &LATENCY_BUCKETS_NS),
            latency_p99_ns: self.latency.percentile(99, &LATENCY_BUCKETS_NS),
            worker_wait_p50_ns: self.worker_wait.percentile(50, &LATENCY_BUCKETS_NS),
            worker_wait_p95_ns: self.worker_wait.percentile(95, &LATENCY_BUCKETS_NS),
            worker_wait_p99_ns: self.worker_wait.percentile(99, &LATENCY_BUCKETS_NS),
            lock_wait_p50_ns: self.lock_wait.percentile(50, &LATENCY_BUCKETS_NS),
            lock_wait_p95_ns: self.lock_wait.percentile(95, &LATENCY_BUCKETS_NS),
            lock_wait_p99_ns: self.lock_wait.percentile(99, &LATENCY_BUCKETS_NS),
            payload_p50_bytes: self.payload.percentile(50, &PAYLOAD_BUCKETS_BYTES),
            payload_p95_bytes: self.payload.percentile(95, &PAYLOAD_BUCKETS_BYTES),
            payload_p99_bytes: self.payload.percentile(99, &PAYLOAD_BUCKETS_BYTES),
        }
    }
}

#[derive(Debug)]
struct FixedHistogram<const N: usize> {
    counts: [u64; N],
    total: u64,
}

impl<const N: usize> Default for FixedHistogram<N> {
    fn default() -> Self {
        Self {
            counts: [0; N],
            total: 0,
        }
    }
}

impl<const N: usize> FixedHistogram<N> {
    fn record(&mut self, value: u64, bounds: &[u64; N]) {
        let index = bounds
            .iter()
            .position(|bound| value <= *bound)
            .unwrap_or(N.saturating_sub(1));
        self.counts[index] = self.counts[index].saturating_add(1);
        self.total = self.total.saturating_add(1);
    }

    fn percentile(&self, percentile: u64, bounds: &[u64; N]) -> u64 {
        if self.total == 0 {
            return 0;
        }
        let target = self.total.saturating_mul(percentile).saturating_add(99) / 100;
        let mut cumulative = 0_u64;
        for (index, count) in self.counts.iter().enumerate() {
            cumulative = cumulative.saturating_add(*count);
            if cumulative >= target {
                return bounds[index];
            }
        }
        bounds[N.saturating_sub(1)]
    }
}

fn increment(value: &mut u64) {
    *value = value.saturating_add(1);
}

fn saturating_increment(counter: &AtomicU64) -> u64 {
    counter
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            Some(value.saturating_add(1))
        })
        .unwrap_or(u64::MAX)
        .saturating_add(1)
}

fn saturating_decrement(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_sub(1))
    });
}

fn update_max(counter: &AtomicU64, candidate: u64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        (candidate > current).then_some(candidate)
    });
}

#[cfg(test)]
#[path = "telemetry_tests.rs"]
mod tests;
