// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/02 21:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/02 21:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Measures bounded direct JSON serialization through the CLI output owner.

#[allow(dead_code, unused_imports)]
#[path = "../src/failure.rs"]
mod failure;
#[allow(dead_code, unused_imports)]
#[path = "../src/output.rs"]
mod output;

use std::hint::black_box;
use std::io::{sink, Write};
use std::time::Instant;

use output::CliOutput;

const JSON_OUTPUT_CASE: &str = "json_stdout_4m";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == JSON_OUTPUT_CASE)
    {
        benchmark_json_output()?;
    }
    if selected
        .as_deref()
        .is_some_and(|value| value != JSON_OUTPUT_CASE)
    {
        return Err(format!("unknown FileMaker CLI benchmark case: {selected:?}").into());
    }
    memory_checkpoint("retained", true);
    Ok(())
}

fn benchmark_json_output() -> Result<(), Box<dyn std::error::Error>> {
    let output = CliOutput::response(
        serde_json::json!({"payload": "o".repeat(4 * 1024 * 1024)}),
        String::new(),
        true,
    );
    measure(JSON_OUTPUT_CASE, 10, || {
        output
            .write_to(&mut sink())
            .map_err(|error| std::io::Error::other(format!("CLI exit {}", error.exit_code())))?;
        black_box(&output);
        Ok(())
    })
}

fn measure(
    case_name: &str,
    fallback_iterations: u64,
    mut operation: impl FnMut() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let iterations = iterations(fallback_iterations);
    memory_checkpoint("workload", true);
    let started = Instant::now();
    for _ in 0..iterations {
        operation()?;
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-filemaker-cli::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn memory_checkpoint(phase: &str, settle: bool) {
    let Some(milliseconds) = std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| (1..=1_000).contains(value))
    else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::stdout().flush();
    if settle {
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
}
