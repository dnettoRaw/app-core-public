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
#[path = "../src/io.rs"]
mod io;
#[allow(dead_code, unused_imports)]
#[path = "../src/output.rs"]
mod output;

use std::fs;
use std::hint::black_box;
use std::io::{sink, Write};
use std::path::Path;
use std::time::Instant;

use failure::{CliFailure, EXIT_IO};
use io::atomic_write;
use output::CliOutput;

const JSON_OUTPUT_CASE: &str = "json_stdout_4m";
const ATOMIC_FILE_CASE: &str = "atomic_file_64m";
const OUTPUT_BYTES: usize = 64 * 1024 * 1024;
const WRITE_BLOCK_BYTES: usize = 64 * 1024;

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
        .is_none_or(|value| value == ATOMIC_FILE_CASE)
    {
        benchmark_atomic_file()?;
    }
    if selected
        .as_deref()
        .is_some_and(|value| ![JSON_OUTPUT_CASE, ATOMIC_FILE_CASE].contains(&value))
    {
        return Err(format!("unknown FileMaker CLI benchmark case: {selected:?}").into());
    }
    memory_checkpoint("retained", true);
    Ok(())
}

fn benchmark_atomic_file() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!(
        "appcore-filemaker-cli-bench-{}.bin",
        std::process::id()
    ));
    let block = [b'x'; WRITE_BLOCK_BYTES];
    let result = measure(ATOMIC_FILE_CASE, 1, || {
        remove_if_present(&path)?;
        atomic_write(&path, false, |writer| {
            for _ in 0..OUTPUT_BYTES / WRITE_BLOCK_BYTES {
                writer.write_all(&block).map_err(|error| {
                    CliFailure::io(
                        EXIT_IO,
                        "FM-CLI-BENCH-IO",
                        format!("cannot write benchmark output: {error}"),
                        false,
                    )
                })?;
            }
            Ok(())
        })
        .map_err(|error| std::io::Error::other(format!("CLI exit {}", error.exit_code())))?;
        let length = fs::metadata(&path)?.len();
        if length != OUTPUT_BYTES as u64 {
            return Err(format!("unexpected benchmark output length: {length}").into());
        }
        remove_if_present(&path)?;
        Ok(())
    });
    let _ = fs::remove_file(&path);
    result
}

fn remove_if_present(path: &Path) -> std::io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
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
