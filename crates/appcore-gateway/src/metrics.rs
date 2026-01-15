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

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Atomic counters representing the live operating metrics of the gateway.
#[derive(Debug, Default)]
pub struct GatewayMetrics {
    active_workers: AtomicU64,
    active_clients: AtomicU64,
    messages_routed: AtomicU64,
    routing_failures: AtomicU64,
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
}

fn saturating_decrement(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
        Some(value.saturating_sub(1))
    });
}
