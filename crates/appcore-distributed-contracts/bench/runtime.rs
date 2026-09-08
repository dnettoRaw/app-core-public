// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures bounded opaque-envelope duplicate admission and retention.

use std::hint::black_box;
use std::time::Instant;

const SMALL_CASE: &str = "deduplicate_32";
const RETAINED_CASE: &str = "deduplicate_65536x128b";
const RETAINED_IDS: usize = 65_536;
const RETAINED_ID_BYTES: usize = 128;

fn main() -> Result<(), String> {
    memory_checkpoint("idle");
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected.as_deref().is_none_or(|value| value == SMALL_CASE) {
        benchmark_small();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == RETAINED_CASE)
    {
        benchmark_retained();
    }
    if let Some(value) = selected.as_deref() {
        if !matches!(value, SMALL_CASE | RETAINED_CASE) {
            return Err(format!(
                "unknown appcore-distributed-contracts benchmark case: {value}"
            ));
        }
    }
    memory_checkpoint("retained");
    Ok(())
}

fn benchmark_small() {
    let iterations = iterations(10_000);
    let started = benchmark_started();
    for _ in 0..iterations {
        let mut deduplicator = appcore_distributed_contracts::OpaqueEnvelopeDeduplicator::new(64);
        for index in 0..32 {
            black_box(deduplicator.accept(&format!("message-{index:02}")));
        }
    }
    report(SMALL_CASE, iterations, started.elapsed().as_nanos());
}

fn benchmark_retained() {
    let identifiers = (0..RETAINED_IDS)
        .map(|index| format!("message-{index:08}-{}", "x".repeat(RETAINED_ID_BYTES - 17)))
        .collect::<Vec<_>>();
    assert!(identifiers
        .iter()
        .all(|value| value.len() == RETAINED_ID_BYTES));
    let iterations = iterations(1);
    let started = benchmark_started();
    for _ in 0..iterations {
        let mut deduplicator =
            appcore_distributed_contracts::OpaqueEnvelopeDeduplicator::new(RETAINED_IDS);
        for identifier in &identifiers {
            black_box(deduplicator.accept(black_box(identifier)));
        }
        black_box(deduplicator);
    }
    report(RETAINED_CASE, iterations, started.elapsed().as_nanos());
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn report(case: &str, iterations: u64, total_ns: u128) {
    println!(
        "appcore-distributed-contracts::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
}

fn benchmark_started() -> Instant {
    memory_checkpoint("workload");
    Instant::now()
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
