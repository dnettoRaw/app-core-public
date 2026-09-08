// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures Runtime availability and observation snapshot retention.

mod metric_generations;

use appcore_core::RuntimeOperationalMode;
use appcore_ops::{
    HealthStatus, InMemoryLogger, InMemoryMetrics, InMemoryObservationSink, LogLevel, LogRecord,
    ObservationEvent, ObservationKind, ObservationSeverity, ObservationSink,
    RuntimeAvailabilityReport, RuntimeLogger,
};
use std::hint::black_box;
use std::sync::Arc;
use std::time::Instant;

const AVAILABILITY_CASE: &str = "availability_projection";
const OBSERVATION_DRAINS_CASE: &str = "observation_emit_32_drains";
const OBSERVATION_SHARED_DRAINS_CASE: &str = "observation_emit_max_payload_32_memory_drains";
const OBSERVATION_OWNED_CASE: &str = "observation_snapshot_owned_1000_of_10000";
const OBSERVATION_SHARED_CASE: &str = "observation_snapshot_shared_1000_of_10000";
const METRIC_OWNED_CASE: &str = "metric_snapshot_owned_1000_of_4096";
const METRIC_SHARED_CASE: &str = "metric_snapshot_shared_1000_of_4096";
const LOG_OWNED_CASE: &str = "log_snapshot_owned_1000_of_4096";
const LOG_SHARED_CASE: &str = "log_snapshot_shared_1000_of_4096";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle");
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    metric_generations::run(selected.as_deref());
    if selected
        .as_deref()
        .is_none_or(|value| value == AVAILABILITY_CASE)
    {
        benchmark_availability();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == OBSERVATION_OWNED_CASE)
    {
        benchmark_observation_snapshot(false);
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == OBSERVATION_SHARED_CASE)
    {
        benchmark_observation_snapshot(true);
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == OBSERVATION_DRAINS_CASE)
    {
        benchmark_observation_drains();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == OBSERVATION_SHARED_DRAINS_CASE)
    {
        benchmark_observation_shared_drains();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == METRIC_OWNED_CASE)
    {
        benchmark_metric_snapshot(false);
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == METRIC_SHARED_CASE)
    {
        benchmark_metric_snapshot(true);
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == LOG_OWNED_CASE)
    {
        benchmark_log_snapshot(false);
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == LOG_SHARED_CASE)
    {
        benchmark_log_snapshot(true);
    }
    if let Some(value) = selected.as_deref() {
        if ![
            AVAILABILITY_CASE,
            OBSERVATION_DRAINS_CASE,
            OBSERVATION_SHARED_DRAINS_CASE,
            OBSERVATION_OWNED_CASE,
            OBSERVATION_SHARED_CASE,
            METRIC_OWNED_CASE,
            METRIC_SHARED_CASE,
            LOG_OWNED_CASE,
            LOG_SHARED_CASE,
        ]
        .contains(&value)
            && !metric_generations::CASES.contains(&value)
        {
            return Err(format!("unknown appcore-ops benchmark case: {value}").into());
        }
    }
    memory_checkpoint("retained");
    Ok(())
}

struct NoopObservationDrain;

impl ObservationSink for NoopObservationDrain {
    fn emit(&self, event: ObservationEvent) {
        black_box(event);
    }
}

fn benchmark_observation_drains() {
    let sink = InMemoryObservationSink::new(1);
    for _ in 0..appcore_ops::MAX_OBSERVATION_DRAINS {
        assert!(sink.try_add_drain(Arc::new(NoopObservationDrain)));
    }
    let event = ObservationEvent::new(
        ObservationKind::Diagnostic,
        ObservationSeverity::Info,
        "runtime.benchmark.drain",
        1,
    );
    measure(OBSERVATION_DRAINS_CASE, 50_000, || {
        sink.emit(black_box(event.clone()));
    });
}

fn benchmark_observation_shared_drains() {
    let sink = InMemoryObservationSink::new(1);
    for _ in 0..appcore_ops::MAX_OBSERVATION_DRAINS {
        assert!(sink.try_add_drain(Arc::new(InMemoryObservationSink::new(1))));
    }
    let mut event = ObservationEvent::new(
        ObservationKind::Diagnostic,
        ObservationSeverity::Info,
        "n".repeat(appcore_ops::MAX_OBSERVATION_NAME_BYTES),
        1,
    )
    .with_trace_id("t".repeat(appcore_ops::MAX_OBSERVATION_TRACE_BYTES));
    for index in 0..appcore_ops::MAX_OBSERVATION_ATTRIBUTES {
        event = event.with_attribute(
            format!("attribute-{index:02}"),
            "v".repeat(appcore_ops::MAX_OBSERVATION_VALUE_BYTES),
        );
    }
    measure(OBSERVATION_SHARED_DRAINS_CASE, 1_000, || {
        sink.emit(black_box(event.clone()));
    });
}

fn benchmark_log_snapshot(shared: bool) {
    let logger = log_fixture();
    let case = if shared {
        LOG_SHARED_CASE
    } else {
        LOG_OWNED_CASE
    };
    measure(case, 1_000, || {
        if shared {
            let snapshot = logger.shared_records();
            for record in snapshot.recent(1_000) {
                black_box(record);
            }
        } else {
            let records = logger.records();
            for record in records.iter().skip(records.len().saturating_sub(1_000)) {
                black_box(record);
            }
        }
    });
}

fn log_fixture() -> InMemoryLogger {
    let logger = InMemoryLogger::new();
    for index in 0..4_096 {
        logger.log(LogRecord {
            level: LogLevel::Info,
            target: "runtime.benchmark".to_string(),
            message: format!("bounded log record {index:04}"),
            timestamp_ms: index,
        });
    }
    assert_eq!(logger.len(), 4_096);
    logger
}

fn benchmark_metric_snapshot(shared: bool) {
    let metrics = metric_fixture();
    let case = if shared {
        METRIC_SHARED_CASE
    } else {
        METRIC_OWNED_CASE
    };
    measure(case, 1_000, || {
        if shared {
            let snapshot = metrics.shared_snapshot();
            for metric in snapshot.iter().skip(snapshot.len().saturating_sub(1_000)) {
                black_box(metric);
            }
        } else {
            let snapshot = metrics.snapshot();
            for metric in snapshot.iter().skip(snapshot.len().saturating_sub(1_000)) {
                black_box(metric);
            }
        }
    });
}

fn metric_fixture() -> InMemoryMetrics {
    let metrics = InMemoryMetrics::new();
    for index in 0..4_096 {
        assert_eq!(metrics.increment(&format!("runtime.metric.{index:04}")), 1);
    }
    assert_eq!(metrics.pressure().entries, 4_096);
    metrics
}

fn benchmark_availability() {
    measure(AVAILABILITY_CASE, 500_000, || {
        black_box(RuntimeAvailabilityReport::evaluate(
            HealthStatus::Healthy,
            RuntimeOperationalMode::ReadWrite,
        ));
    });
}

fn benchmark_observation_snapshot(shared: bool) {
    let sink = observation_fixture();
    let case = if shared {
        OBSERVATION_SHARED_CASE
    } else {
        OBSERVATION_OWNED_CASE
    };
    measure(case, 1_000, || {
        if shared {
            let snapshot = sink.shared_snapshot();
            for event in snapshot.recent(1_000) {
                black_box(event);
            }
        } else {
            let snapshot = sink.snapshot();
            for event in snapshot.iter().skip(snapshot.len().saturating_sub(1_000)) {
                black_box(event);
            }
        }
    });
}

fn observation_fixture() -> InMemoryObservationSink {
    let sink = InMemoryObservationSink::new(10_000);
    for index in 0..10_000 {
        sink.emit(ObservationEvent::new(
            ObservationKind::Diagnostic,
            ObservationSeverity::Info,
            format!("runtime.benchmark.{index:05}"),
            index,
        ));
    }
    assert_eq!(sink.len(), 10_000);
    sink
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn measure(case: &str, fallback: u64, mut operation: impl FnMut()) {
    let iterations = iterations(fallback);
    memory_checkpoint("workload");
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-ops::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
}
fn memory_checkpoint(phase: &str) {
    let Some(milliseconds) = checkpoint_milliseconds() else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::thread::sleep(std::time::Duration::from_millis(milliseconds));
}
fn checkpoint_milliseconds() -> Option<u64> {
    std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()?
        .parse()
        .ok()
        .filter(|value| (1..=1_000).contains(value))
}
