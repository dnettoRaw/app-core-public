// =============================================================================
//        #######
//     ###       ###     F: telemetry_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.5.0-alpha.1
// =============================================================================
// appcore-norm: test

use super::*;
use crate::GatewayMetrics;
use appcore_types::CapabilityName;
use std::sync::Arc;
use std::time::Duration;

#[test]
fn route_snapshot_uses_fixed_buckets_and_outcomes() {
    let metrics = GatewayMetrics::new();
    let capability = CapabilityName::new("runtime.telemetry.test").unwrap();
    {
        let mut route = metrics.route_started(Some(&capability), 2_048);
        route.record_lock_wait(Duration::from_micros(150));
        route.observe_queue_depth(7);
        route.dispatched();
        route.finish(RouteOutcome::Success);
    }

    let snapshot = metrics.telemetry_snapshot();
    assert_eq!(snapshot.inflight, 0);
    assert_eq!(snapshot.inflight_peak, 1);
    assert_eq!(snapshot.queue_depth_peak, 7);
    assert_eq!(snapshot.capabilities.len(), 1);
    let series = &snapshot.capabilities[0];
    assert_eq!(series.capability, capability.as_str());
    assert_eq!(series.requests, 1);
    assert_eq!(series.successes, 1);
    assert_eq!(series.payload_p50_bytes, 4_096);
    assert_eq!(series.lock_wait_p50_ns, 500_000);
    assert!(series.latency_p99_ns > 0);
}

#[test]
fn capability_cardinality_overflows_into_one_fixed_series() {
    let metrics = GatewayMetrics::new();
    for index in 0..(MAX_GATEWAY_TELEMETRY_CAPABILITIES + 5) {
        let capability = CapabilityName::new(format!("runtime.telemetry.{index}")).unwrap();
        let mut route = metrics.route_started(Some(&capability), 0);
        route.finish(RouteOutcome::WorkerUnavailable);
    }

    let snapshot = metrics.telemetry_snapshot();
    assert_eq!(
        snapshot.capabilities.len(),
        MAX_GATEWAY_TELEMETRY_CAPABILITIES + 1
    );
    assert_eq!(snapshot.capability_overflow, 5);
    let overflow = snapshot
        .capabilities
        .iter()
        .find(|series| series.capability == OVERFLOW_CAPABILITY)
        .unwrap();
    assert_eq!(overflow.requests, 5);
    assert_eq!(overflow.worker_unavailable, 5);
}

#[test]
fn concurrent_updates_remain_bounded_and_do_not_lose_inflight() {
    let metrics = GatewayMetrics::new();
    let mut workers = Vec::new();
    for _ in 0..8 {
        let metrics = Arc::clone(&metrics);
        workers.push(std::thread::spawn(move || {
            let capability = CapabilityName::new("runtime.telemetry.concurrent").unwrap();
            for _ in 0..100 {
                let mut route = metrics.route_started(Some(&capability), 64);
                route.finish(RouteOutcome::Success);
            }
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }

    let snapshot = metrics.telemetry_snapshot();
    assert_eq!(snapshot.inflight, 0);
    assert_eq!(snapshot.capabilities[0].requests, 800);
    assert_eq!(snapshot.capabilities[0].successes, 800);
}

#[test]
fn exporter_failure_is_visible_and_does_not_mutate_route_data() {
    struct FailingExporter;

    impl GatewayTelemetryExporter for FailingExporter {
        fn export(
            &self,
            _snapshot: &GatewayTelemetrySnapshot,
        ) -> Result<(), GatewayTelemetryExportError> {
            Err(GatewayTelemetryExportError)
        }
    }

    let metrics = GatewayMetrics::new();
    assert_eq!(
        metrics.export_telemetry(&FailingExporter),
        Err(GatewayTelemetryExportError)
    );
    let snapshot = metrics.telemetry_snapshot();
    assert_eq!(snapshot.export_failures, 1);
    assert!(snapshot.capabilities.is_empty());
}
