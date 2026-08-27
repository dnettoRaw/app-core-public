// =============================================================================
//        #######
//     ###       ###     F: metrics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Lightweight observability metrics for Gateway health and throughput.

use crate::telemetry::{
    GatewayTelemetryExportError, GatewayTelemetryExporter, GatewayTelemetryRegistry,
    GatewayTelemetrySnapshot, RouteOutcome, RouteSample,
};
use appcore_types::CapabilityName;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

const MESH_ROUTE_CAPABILITY: &str = "runtime.gateway.mesh";

/// Atomic counters representing the live operating metrics of the gateway.
#[derive(Debug, Default)]
pub struct GatewayMetrics {
    active_workers: AtomicU64,
    active_clients: AtomicU64,
    messages_routed: AtomicU64,
    routing_failures: AtomicU64,
    telemetry: GatewayTelemetryRegistry,
}

impl GatewayMetrics {
    /// Creates a thread-safe handle for metrics tracking.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Increments the active worker connection count.
    pub fn worker_connected(&self) {
        self.active_workers.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrements the active worker connection count.
    pub fn worker_disconnected(&self) {
        saturating_decrement(&self.active_workers);
    }

    /// Increments the active client connection count.
    pub fn client_connected(&self) {
        self.active_clients.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrements the active client connection count.
    pub fn client_disconnected(&self) {
        saturating_decrement(&self.active_clients);
    }

    /// Records a successfully routed envelope.
    pub fn message_routed(&self) {
        self.messages_routed.fetch_add(1, Ordering::Relaxed);
    }

    /// Records a failure to route an envelope.
    pub fn routing_failure(&self) {
        self.routing_failures.fetch_add(1, Ordering::Relaxed);
    }

    /// Returns the snapshot of active workers.
    pub fn active_workers(&self) -> u64 {
        self.active_workers.load(Ordering::Relaxed)
    }

    /// Returns the snapshot of active clients.
    pub fn active_clients(&self) -> u64 {
        self.active_clients.load(Ordering::Relaxed)
    }

    /// Returns the total messages routed.
    pub fn messages_routed(&self) -> u64 {
        self.messages_routed.load(Ordering::Relaxed)
    }

    /// Returns the total routing failures.
    pub fn routing_failures(&self) -> u64 {
        self.routing_failures.load(Ordering::Relaxed)
    }

    /// Returns detailed bounded vendor-neutral route telemetry.
    pub fn telemetry_snapshot(&self) -> GatewayTelemetrySnapshot {
        self.telemetry.snapshot()
    }

    /// Invokes an explicit deployment-owned exporter with an immutable
    /// snapshot and records a controlled failure without affecting routing.
    pub fn export_telemetry(
        &self,
        exporter: &dyn GatewayTelemetryExporter,
    ) -> Result<(), GatewayTelemetryExportError> {
        let snapshot = self.telemetry_snapshot();
        exporter.export(&snapshot).inspect_err(|_| {
            self.telemetry.export_failure();
        })
    }

    /// Records an explicit Gateway route retry attempt.
    pub fn route_retried(&self) {
        self.telemetry.retry();
    }

    pub(crate) fn route_started(
        self: &Arc<Self>,
        capability: Option<&CapabilityName>,
        payload_bytes: usize,
    ) -> RouteObservation {
        self.telemetry.route_started();
        RouteObservation {
            metrics: Arc::clone(self),
            capability: capability.map_or_else(
                || MESH_ROUTE_CAPABILITY.to_string(),
                |value| value.as_str().to_string(),
            ),
            started: Instant::now(),
            dispatched: None,
            lock_wait_ns: 0,
            payload_bytes: u64::try_from(payload_bytes).unwrap_or(u64::MAX),
            outcome: RouteOutcome::TransportFailure,
        }
    }

    pub(crate) fn worker_reconnected(&self) {
        self.telemetry.reconnect();
    }

    pub(crate) fn worker_route_admitted(&self, inflight: u64) {
        self.telemetry.observe_worker_inflight(inflight);
    }

    pub(crate) fn authentication_failure(&self) {
        self.telemetry.authentication_failure();
    }
}

pub(crate) struct RouteObservation {
    metrics: Arc<GatewayMetrics>,
    capability: String,
    started: Instant,
    dispatched: Option<Instant>,
    lock_wait_ns: u64,
    payload_bytes: u64,
    outcome: RouteOutcome,
}

impl RouteObservation {
    pub(crate) fn set_capability(&mut self, capability: &CapabilityName) {
        self.capability = capability.as_str().to_string();
    }

    pub(crate) fn record_lock_wait(&mut self, wait: Duration) {
        self.lock_wait_ns = self.lock_wait_ns.saturating_add(duration_ns(wait));
    }

    pub(crate) fn observe_queue_depth(&self, depth: usize) {
        self.metrics.telemetry.observe_queue_depth(depth);
    }

    pub(crate) fn dispatched(&mut self) {
        self.dispatched = Some(Instant::now());
    }

    pub(crate) fn finish(&mut self, outcome: RouteOutcome) {
        self.outcome = outcome;
    }
}

impl Drop for RouteObservation {
    fn drop(&mut self) {
        self.metrics.telemetry.route_finished(RouteSample {
            capability: self.capability.clone(),
            outcome: self.outcome,
            latency_ns: duration_ns(self.started.elapsed()),
            worker_wait_ns: self
                .dispatched
                .map(|started| duration_ns(started.elapsed())),
            lock_wait_ns: self.lock_wait_ns,
            payload_bytes: self.payload_bytes,
        });
    }
}

fn saturating_decrement(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_sub(1))
    });
}

fn duration_ns(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}
