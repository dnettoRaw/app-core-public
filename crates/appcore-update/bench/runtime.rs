// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures signing payload construction and bounded file activation.

use appcore_contracts::{ApplicationId, BuildId};
use appcore_update::{
    artifact_signing_payload, ArtifactDescriptor, ArtifactStore, FileArtifactStore,
};
use sha2::{Digest, Sha256};
use std::hint::black_box;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

const SIGNING_CASE: &str = "artifact_signing_payload";
const FILE_ACTIVATION_CASE: &str = "file_stage_activate_8mib";
const BENCHMARK_ARTIFACT_BYTES: usize = 8 * 1024 * 1024;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == SIGNING_CASE)
    {
        benchmark_signing_payload()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == FILE_ACTIVATION_CASE)
    {
        benchmark_file_activation()?;
    }
    if let Some(value) = selected.as_deref() {
        if value != SIGNING_CASE && value != FILE_ACTIVATION_CASE {
            return Err(format!("unknown appcore-update benchmark case: {value}").into());
        }
    }
    memory_checkpoint("retained", true);
    Ok(())
}

fn benchmark_signing_payload() -> Result<(), Box<dyn std::error::Error>> {
    let artifact = ArtifactDescriptor::new(
        ApplicationId::new("bench.application")?,
        "1.2.3",
        BuildId::new("build-bench")?,
        "stable",
        ">=1.0.0",
        "1",
        "provider://bench/artifact",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        4_096,
    )?;
    measure(SIGNING_CASE, 50_000, || {
        black_box(artifact_signing_payload(black_box(&artifact)));
        Ok(())
    })
}

fn benchmark_file_activation() -> Result<(), Box<dyn std::error::Error>> {
    let root = benchmark_root();
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    let store = FileArtifactStore::new(&root);
    let bytes = (0..BENCHMARK_ARTIFACT_BYTES)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let artifact = ArtifactDescriptor::new(
        ApplicationId::new("bench.application")?,
        "1.2.3",
        BuildId::new("build-file-activation")?,
        "stable",
        ">=1.0.0",
        "1",
        "provider://bench/artifact",
        format!("{:x}", Sha256::digest(&bytes)),
        bytes.len() as u64,
    )?;
    let result = measure(FILE_ACTIVATION_CASE, 1, || {
        let staged = store.stage(&artifact, &bytes)?;
        let receipt = store.activate(staged)?;
        store.commit(&receipt)?;
        black_box(receipt);
        Ok(())
    });
    std::fs::remove_dir_all(root)?;
    result
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
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
        "appcore-update::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

fn benchmark_root() -> PathBuf {
    std::env::temp_dir().join(format!("appcore-update-benchmark-{}", std::process::id()))
}

fn memory_checkpoint(phase: &str, settle: bool) {
    let Some(milliseconds) = checkpoint_milliseconds() else {
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
fn checkpoint_milliseconds() -> Option<u64> {
    std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()?
        .parse()
        .ok()
        .filter(|value| (1..=1_000).contains(value))
}
