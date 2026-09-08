// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures validated identifiers, bounded registry growth, and artifact activation.

use appcore_ai::{
    AiError, AiLimits, AiModality, AiRequest, AiTask, ArtifactDigest, ArtifactFormat,
    ArtifactIdentity, ArtifactLocation, BackendId, CancellationToken, DeviceKind,
    LightweightEngine, LightweightResolver, LocalArtifactCache, ModelDescriptor, ModelId,
    ModelRegistry, PeerId, QualityTier, Quantization,
};
use std::hint::black_box;
use std::path::PathBuf;
use std::time::Instant;

const MODEL_ID_CASE: &str = "model_id_validation";
const LIGHTWEIGHT_NORMALIZE_CASE: &str = "lightweight_normalize_1mib_words";
const MODEL_LOCATION_CASE: &str = "model_registry_location_pressure_65536";
const ARTIFACT_STORE_CASE: &str = "idempotent_artifact_store_32mib";
const ARTIFACT_BYTES: usize = 32 * 1024 * 1024;
const MODEL_LOCATIONS: usize = 65_536;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected.as_deref().is_none_or(|case| case == MODEL_ID_CASE) {
        benchmark_model_id();
    }
    if selected
        .as_deref()
        .is_none_or(|case| case == LIGHTWEIGHT_NORMALIZE_CASE)
    {
        benchmark_lightweight_normalize()?;
    }
    if selected
        .as_deref()
        .is_none_or(|case| case == MODEL_LOCATION_CASE)
    {
        benchmark_model_locations()?;
    }
    if selected
        .as_deref()
        .is_none_or(|case| case == ARTIFACT_STORE_CASE)
    {
        benchmark_artifact_store()?;
    }
    if let Some(case) = selected.as_deref() {
        if case != MODEL_ID_CASE
            && case != LIGHTWEIGHT_NORMALIZE_CASE
            && case != MODEL_LOCATION_CASE
            && case != ARTIFACT_STORE_CASE
        {
            return Err(format!("unknown appcore-ai benchmark case: {case}").into());
        }
    }
    Ok(())
}

fn benchmark_lightweight_normalize() -> Result<(), Box<dyn std::error::Error>> {
    let limits = AiLimits::default();
    let engine = LightweightEngine::new(Vec::new(), limits, 0)?;
    let input = "x ".repeat(limits.max_input_bytes / 2);
    let request = AiRequest::text(AiTask::TransformText, input, limits)?;
    let cancellation = CancellationToken::new();
    let iterations = iterations(1);
    let mut retained = None;
    memory_checkpoint("idle");
    let started = benchmark_started();
    for _ in 0..iterations {
        retained = Some(engine.resolve(&request, &cancellation)?);
    }
    report(
        LIGHTWEIGHT_NORMALIZE_CASE,
        iterations,
        started.elapsed().as_nanos(),
    );
    std::hint::black_box(retained);
    Ok(())
}

fn benchmark_model_locations() -> Result<(), Box<dyn std::error::Error>> {
    let descriptor = benchmark_descriptor()?;
    let iterations = iterations(1);
    let mut retained = ModelRegistry::new();
    retained.register(descriptor.clone(), [])?;
    memory_checkpoint("idle");
    let started = benchmark_started();
    let mut accepted = 0usize;
    let mut rejected = 0usize;
    for _ in 0..iterations {
        let registry = ModelRegistry::new();
        registry.register(descriptor.clone(), [])?;
        accepted = 0;
        rejected = 0;
        for index in 0..MODEL_LOCATIONS {
            let location = ArtifactLocation::Peer(PeerId::new(format!("bench/peer-{index}"))?);
            match registry.add_location(&descriptor.id, location) {
                Ok(()) => accepted = accepted.saturating_add(1),
                Err(AiError::Capacity(_)) => rejected = rejected.saturating_add(1),
                Err(error) => return Err(error.into()),
            }
        }
        retained = registry;
    }
    let pressure = retained.pressure();
    if accepted != pressure.max_locations_per_model
        || rejected != MODEL_LOCATIONS.saturating_sub(pressure.max_locations_per_model)
        || pressure.current_locations != pressure.max_locations_per_model
    {
        return Err("model registry location bounds were not enforced".into());
    }
    black_box(pressure);
    report(
        MODEL_LOCATION_CASE,
        iterations,
        started.elapsed().as_nanos(),
    );
    black_box(retained.snapshot()?);
    Ok(())
}

fn benchmark_descriptor() -> Result<ModelDescriptor, AiError> {
    let id = ModelId::new("bench/model-location-pressure")?;
    Ok(ModelDescriptor {
        id,
        revision: "v1".to_string(),
        tasks: vec![AiTask::GenerateText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::Gguf,
        quantization: Quantization::Int4,
        estimated_memory_bytes: 1,
        estimated_vram_bytes: 0,
        max_input_bytes: 1,
        max_output_bytes: 1,
        context_limit: None,
        supported_backends: vec![BackendId::new("bench/backend")?],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 1,
        quality: Some(QualityTier::Tiny),
        artifact: ArtifactIdentity {
            digest: ArtifactDigest::from_bytes(b"bench/model-location-pressure"),
            size_bytes: 1,
            publisher: None,
            signature_required: false,
        },
    })
}

fn benchmark_model_id() {
    memory_checkpoint("idle");
    let iterations = iterations(100_000);
    let started = benchmark_started();
    for _ in 0..iterations {
        let _ = black_box(appcore_ai::ModelId::new("bench/model-01"));
    }
    report(MODEL_ID_CASE, iterations, started.elapsed().as_nanos());
}

fn benchmark_artifact_store() -> Result<(), Box<dyn std::error::Error>> {
    let root = benchmark_root();
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    let bytes = vec![0x5a; ARTIFACT_BYTES];
    let identity = ArtifactIdentity {
        digest: ArtifactDigest::from_bytes(&bytes),
        size_bytes: bytes.len() as u64,
        publisher: None,
        signature_required: false,
    };
    let cache = LocalArtifactCache::new(&root, identity.size_bytes)?;
    cache.store(&identity, &bytes)?;
    memory_checkpoint("idle");
    let iterations = iterations(1);
    let started = benchmark_started();
    for _ in 0..iterations {
        black_box(cache.store(black_box(&identity), black_box(&bytes))?);
    }
    report(
        ARTIFACT_STORE_CASE,
        iterations,
        started.elapsed().as_nanos(),
    );
    std::fs::remove_dir_all(root)?;
    Ok(())
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
        "appcore-ai::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    memory_checkpoint("retained");
}

fn benchmark_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-ai-artifact-benchmark-{}",
        std::process::id()
    ))
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
