// =============================================================================
//        #######
//     ###       ###     F: candle_load_memory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.3
// =============================================================================

//! Measures peak memory while loading a representative 4 MiB Candle classifier.

use appcore_ai::{
    AiModality, AiTask, ArtifactFormat, ArtifactStore, BackendId, CancellationToken, CandleBackend,
    CandleBackendConfig, DeviceKind, InferenceBackend, MemoryArtifactStore, ModelDescriptor,
    ModelId, NativeLinearArtifact, QualityTier, Quantization, CANDLE_LINEAR_BACKEND_ID,
};
use std::future::Future;
use std::hint::black_box;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Instant;

const CASE: &str = "candle_model_load_4096x256";
const DIMENSIONS: usize = 4_096;
const CLASSES: usize = 256;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let labels = (0..CLASSES)
        .map(|index| format!("class-{index}"))
        .collect::<Vec<_>>();
    let artifact = NativeLinearArtifact::new(
        DIMENSIONS,
        labels.clone(),
        vec![0.001; DIMENSIONS * CLASSES],
        vec![0.0; CLASSES],
    )?;
    let bytes = artifact.encode()?;
    let identity = artifact.identity(None, false)?;
    let descriptor = ModelDescriptor {
        id: ModelId::new("bench/candle-4096x256")?,
        revision: "v1".to_string(),
        tasks: vec![AiTask::ClassifyText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::NativeLinearV1,
        quantization: Quantization::None,
        estimated_memory_bytes: identity.size_bytes.saturating_mul(2),
        estimated_vram_bytes: 0,
        max_input_bytes: 1_024,
        max_output_bytes: 64 * 1_024,
        context_limit: None,
        supported_backends: vec![BackendId::new(CANDLE_LINEAR_BACKEND_ID)?],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 1,
        quality: Some(QualityTier::Tiny),
        artifact: identity.clone(),
    };
    let store = Arc::new(MemoryArtifactStore::new(
        identity.size_bytes.saturating_mul(2),
    )?);
    store.store(&identity, &bytes, &CancellationToken::new())?;
    drop((artifact, bytes, labels));
    let iterations = iterations();
    let mut retained = CandleBackend::new(store.clone(), CandleBackendConfig::default())?;
    memory_checkpoint("idle");
    let started = benchmark_started();
    for _ in 0..iterations {
        let backend = CandleBackend::new(store.clone(), CandleBackendConfig::default())?;
        block_on(backend.load(&descriptor, &CancellationToken::new()))?;
        retained = backend;
    }
    report(iterations, started.elapsed().as_nanos());
    black_box(retained);
    Ok(())
}

fn iterations() -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(1)
}

fn report(iterations: u64, total_ns: u128) {
    println!(
        "appcore-ai::{CASE} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
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

fn block_on<F: Future>(future: F) -> F::Output {
    let mut context = Context::from_waker(Waker::noop());
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
