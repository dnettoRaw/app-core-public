// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures stable application identifier validation.

use std::hint::black_box;
use std::time::Instant;

fn main() {
    memory_checkpoint("idle");
    let iterations = iterations(100_000);
    let started = benchmark_started();
    for _ in 0..iterations {
        let _ = black_box(appcore_contracts::ApplicationId::new(
            "bench.application-01",
        ));
    }
    report(iterations, started.elapsed().as_nanos());
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn report(iterations: u64, total_ns: u128) {
    println!(
        "appcore-contracts::application_id_validation iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    memory_checkpoint("retained");
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
