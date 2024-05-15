// =============================================================================
//        #######
//     ###       ###     F: observation_metrics.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Low-cardinality metrics derived from runtime observations.

use crate::{
    InMemoryMetrics, ObservationEvent, ObservationKind, ObservationSeverity, ObservationSink,
};
use std::sync::Arc;

/// Observation drain that records stable monotonic counters.
#[derive(Debug, Clone)]
pub struct ObservationMetricsSink {
    metrics: Arc<InMemoryMetrics>,
}

impl ObservationMetricsSink {
    /// Creates a drain backed by the provided process-local registry.
    pub fn new(metrics: Arc<InMemoryMetrics>) -> Self {
        Self { metrics }
    }

    /// Returns the shared metrics registry.
    pub fn metrics(&self) -> Arc<InMemoryMetrics> {
        Arc::clone(&self.metrics)
    }
}

impl ObservationSink for ObservationMetricsSink {
    fn emit(&self, event: ObservationEvent) {
        let _ = self.metrics.increment("appcore.observations.total");
        let _ = self.metrics.increment(kind_metric(event.kind));
        let _ = self.metrics.increment(severity_metric(event.severity));
    }
}

fn kind_metric(kind: ObservationKind) -> &'static str {
    match kind {
        ObservationKind::Lifecycle => "appcore.observations.kind.lifecycle",
        ObservationKind::Configuration => "appcore.observations.kind.configuration",
        ObservationKind::Health => "appcore.observations.kind.health",
        ObservationKind::Security => "appcore.observations.kind.security",
        ObservationKind::Storage => "appcore.observations.kind.storage",
        ObservationKind::ControlPlane => "appcore.observations.kind.control_plane",
        ObservationKind::PeerRpc => "appcore.observations.kind.peer_rpc",
        ObservationKind::Scheduler => "appcore.observations.kind.scheduler",
        ObservationKind::Sync => "appcore.observations.kind.sync",
        ObservationKind::Audit => "appcore.observations.kind.audit",
        ObservationKind::Diagnostic => "appcore.observations.kind.diagnostic",
    }
}

fn severity_metric(severity: ObservationSeverity) -> &'static str {
    match severity {
        ObservationSeverity::Debug => "appcore.observations.severity.debug",
        ObservationSeverity::Info => "appcore.observations.severity.info",
        ObservationSeverity::Warning => "appcore.observations.severity.warning",
        ObservationSeverity::Error => "appcore.observations.severity.error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_bounded_kind_and_severity_dimensions() {
        let metrics = Arc::new(InMemoryMetrics::new());
        let sink = ObservationMetricsSink::new(Arc::clone(&metrics));
        sink.emit(ObservationEvent::new(
            ObservationKind::Storage,
            ObservationSeverity::Warning,
            "untrusted.dynamic.name",
            1,
        ));

        let snapshot = metrics.snapshot();
        assert!(snapshot
            .iter()
            .any(|metric| metric.name == "appcore.observations.total" && metric.value == 1));
        assert!(snapshot.iter().any(|metric| {
            metric.name == "appcore.observations.kind.storage" && metric.value == 1
        }));
        assert!(snapshot.iter().any(|metric| {
            metric.name == "appcore.observations.severity.warning" && metric.value == 1
        }));
        assert_eq!(snapshot.len(), 3);
    }
}
