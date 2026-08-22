// =============================================================================
//        #######
//     ###       ###     F: perf_lab.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/22 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/22 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::*;
use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

const FAST_ITERATIONS: usize = 2_000;
const ROUTE_ITERATIONS: usize = 1_000;
const COLD_ITERATIONS: usize = 128;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let format = OutputFormat::from_environment();
    benchmark_resolve(&format)?;
    benchmark_resources(&format)?;
    benchmark_request_validation(&format)?;
    benchmark_registries(&format)?;
    benchmark_scheduler(&format)?;
    benchmark_batching(&format)?;
    benchmark_residency(&format)?;
    benchmark_artifacts(&format)?;
    #[cfg(feature = "backend-candle")]
    benchmark_candle_batches(&format)?;
    #[cfg(feature = "training-candle")]
    benchmark_candle_training(&format)?;
    #[cfg(feature = "swarm")]
    benchmark_swarm(&format)?;
    Ok(())
}

fn benchmark_resources(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let cached = SystemHardwareProbe::default();
    cached.sample()?;
    report(
        format,
        "resource_snapshot_cached",
        measure(FAST_ITERATIONS, || {
            std::hint::black_box(cached.sample()?);
            Ok(())
        })?,
    );
    let dynamic = SystemHardwareProbe::with_sampling_interval(Duration::from_secs(1))?;
    report(
        format,
        "resource_dynamic_sample",
        measure(COLD_ITERATIONS, || {
            std::hint::black_box(dynamic.refresh()?);
            Ok(())
        })?,
    );
    report(
        format,
        "resource_static_discovery",
        measure(COLD_ITERATIONS, || {
            std::hint::black_box(SystemHardwareProbe::with_sampling_interval(
                Duration::from_secs(1),
            )?);
            Ok(())
        })?,
    );
    Ok(())
}

fn benchmark_request_validation(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let limits = AiLimits::default();
    let request = AiRequest {
        task: AiTask::AnalyzeImage,
        input: AiInput::new(
            vec![AiContent::Binary {
                media_type: "image/png".into(),
                bytes: vec![0x5a; limits.max_input_bytes - "image/png".len()],
            }],
            limits,
        )?,
        options: AiOptions::default(),
    };
    report(
        format,
        "request_validate_binary_1mib_borrowed",
        measure(FAST_ITERATIONS, || {
            request.validate(limits)?;
            Ok(())
        })?,
    );
    report(
        format,
        "request_validate_binary_1mib_clone_control",
        measure(COLD_ITERATIONS, || {
            std::hint::black_box(request.clone()).validate(limits)?;
            Ok(())
        })?,
    );
    Ok(())
}

fn benchmark_resolve(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let lightweight = runtime_with_backends(0, 0)?;
    report(
        format,
        "resolve_lightweight_hit",
        measure(FAST_ITERATIONS, || {
            block_on(lightweight.resolve(AiRequest::text(
                AiTask::TransformText,
                " bounded   text ",
                AiLimits::default(),
            )?))?;
            Ok(())
        })?,
    );

    let missing = runtime_with_backends(0, 0)?;
    report(
        format,
        "resolve_model_miss",
        measure(FAST_ITERATIONS, || {
            let result = block_on(missing.resolve(AiRequest::text(
                AiTask::GenerateText,
                "bounded prompt",
                AiLimits::default(),
            )?));
            if !matches!(result, Err(AiError::NotFound("compatible AI route"))) {
                return Err("unexpected missing-route result".into());
            }
            Ok(())
        })?,
    );

    let warm = runtime_with_backends(1, 1)?;
    let request = forced_request("perf/model-0")?;
    block_on(warm.resolve(request.clone()))?;
    report(
        format,
        "resolve_backend_warm_1_route",
        measure(ROUTE_ITERATIONS, || {
            block_on(warm.resolve(request.clone()))?;
            Ok(())
        })?,
    );

    let many_routes = runtime_with_backends(1, 32)?;
    let request = forced_request("perf/model-0")?;
    block_on(many_routes.resolve(request.clone()))?;
    report(
        format,
        "resolve_backend_warm_32_routes",
        measure(ROUTE_ITERATIONS, || {
            block_on(many_routes.resolve(request.clone()))?;
            Ok(())
        })?,
    );

    let cold = runtime_with_backends(COLD_ITERATIONS, 1)?;
    let mut index = 0usize;
    report(
        format,
        "resolve_backend_cold_unique_model",
        measure(COLD_ITERATIONS, || {
            let request = forced_request(&format!("perf/model-{index}"))?;
            index = index.saturating_add(1);
            block_on(cold.resolve(request))?;
            Ok(())
        })?,
    );
    Ok(())
}

fn benchmark_registries(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    for count in [1usize, 32, 128] {
        let registry = ModelRegistry::new();
        for index in 0..count {
            registry.register(
                model_descriptor(index, &[BackendId::new("perf/backend-0")?])?,
                [ArtifactLocation::Memory],
            )?;
        }
        report(
            format,
            &format!("model_registry_candidates_{count}"),
            measure(FAST_ITERATIONS, || {
                std::hint::black_box(registry.candidates(&AiTask::GenerateText)?);
                Ok(())
            })?,
        );
    }
    Ok(())
}

fn benchmark_scheduler(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let scheduler = CostScheduler::default();
    for count in [1usize, 32, 128] {
        let candidates = placement_candidates(count)?;
        report(
            format,
            &format!("scheduler_{count}_candidates"),
            measure(FAST_ITERATIONS, || {
                std::hint::black_box(scheduler.plan(placement_context(false), &candidates));
                Ok(())
            })?,
        );
    }
    Ok(())
}

fn benchmark_batching(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    for size in [1usize, 2, 4, 8, 16] {
        report(
            format,
            &format!("batch_enqueue_flush_{size}"),
            measure(FAST_ITERATIONS, || {
                let mut batcher = DynamicBatcher::new(DynamicBatcherConfig {
                    max_queues: 1,
                    max_total_items: 16,
                    max_queue_depth: 16,
                    max_batch_size: size,
                    max_wait: Duration::from_millis(1),
                    overload_retry_after: Duration::from_millis(1),
                })?;
                let key = batch_key()?;
                for item in 0..size {
                    std::hint::black_box(batcher.enqueue(
                        key.clone(),
                        item,
                        0,
                        Some(Duration::from_secs(1)),
                        CancellationToken::new(),
                    ));
                }
                std::hint::black_box(batcher.take_ready(
                    &key,
                    0,
                    BatchPressure {
                        resource_mode: AiResourceMode::Performance,
                        pressure_limited: false,
                        available_memory_bytes: Some(1 << 20),
                        available_vram_bytes: Some(0),
                        estimated_item_memory_bytes: 1,
                        estimated_item_vram_bytes: 0,
                        device_load_percent: Some(0),
                    },
                    false,
                ));
                Ok(())
            })?,
        );
    }
    Ok(())
}

fn benchmark_residency(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let planner = ResidencyPlanner::new(
        ResidencyConfig::default(),
        vec![TierCapacity {
            tier: ResidencyTier::Memory,
            capacity_bytes: 1 << 20,
        }],
    )?;
    let mut sequence = 0usize;
    report(
        format,
        "residency_cold_reserve_rollback",
        measure(FAST_ITERATIONS, || {
            let decision = planner.begin(ResidencyRequest {
                model: ModelId::new(format!("perf/residency-{sequence}"))?,
                preferred: ResidencyTier::Memory,
                fallbacks: Vec::new(),
                size_bytes: 1_024,
                load_time_ms: 1,
                importance_basis_points: 1_000,
                predicted_next_use_ms: None,
                now_ms: u64::try_from(sequence)?,
                resource_mode: AiResourceMode::Balanced,
                capacity_limit_bytes: None,
                prefetch: false,
                cancellation: CancellationToken::new(),
            })?;
            let ResidencyDecision::Reserved(reservation) = decision else {
                return Err("unexpected residency decision".into());
            };
            planner.finish(reservation, false, u64::try_from(sequence)?)?;
            sequence = sequence.saturating_add(1);
            Ok(())
        })?,
    );
    Ok(())
}

fn benchmark_artifacts(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let root = std::env::temp_dir().join(format!(
        "appcore-ai-perf-{}-{}",
        std::process::id(),
        Instant::now().elapsed().as_nanos()
    ));
    let bytes = vec![0x5a; 1024 * 1024];
    let identity = ArtifactIdentity {
        digest: ArtifactDigest::from_bytes(&bytes),
        size_bytes: u64::try_from(bytes.len())?,
        publisher: None,
        signature_required: false,
    };
    let cache = LocalArtifactCache::new(&root, 2 * 1024 * 1024)?;
    cache.store(&identity, &bytes)?;
    report(
        format,
        "artifact_local_full_1mib",
        measure(COLD_ITERATIONS, || {
            std::hint::black_box(cache.load(&identity)?);
            Ok(())
        })?,
    );
    report(
        format,
        "artifact_local_range_4kib",
        measure(FAST_ITERATIONS, || {
            std::hint::black_box(cache.load_range(&identity, 4096, 4096, 4096)?);
            Ok(())
        })?,
    );
    std::fs::remove_dir_all(&root)?;
    Ok(())
}

#[cfg(feature = "backend-candle")]
fn benchmark_candle_batches(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let dimensions = 64usize;
    let artifact = NativeLinearArtifact::new(
        dimensions,
        vec!["a".into(), "b".into()],
        vec![0.01; dimensions * 2],
        vec![0.0; 2],
    )?;
    let bytes = artifact.encode()?;
    let identity = artifact.identity(None, false)?;
    let memory = Arc::new(MemoryArtifactStore::new(1 << 20)?);
    memory.store(&identity, &bytes, &CancellationToken::new())?;
    let store: Arc<dyn ArtifactStore> = memory;
    let backend = CandleBackend::new(store, CandleBackendConfig::default())?;
    let descriptor = ModelDescriptor {
        id: ModelId::new("perf/candle-batch")?,
        revision: "v1".into(),
        tasks: vec![AiTask::ClassifyText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::NativeLinearV1,
        quantization: Quantization::None,
        estimated_memory_bytes: 1_024,
        estimated_vram_bytes: 0,
        max_input_bytes: 1_024,
        max_output_bytes: 1_024,
        context_limit: None,
        supported_backends: vec![BackendId::new(CANDLE_LINEAR_BACKEND_ID)?],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 1,
        quality: Some(QualityTier::Tiny),
        artifact: identity,
    };
    block_on(backend.load(&descriptor, &CancellationToken::new()))?;
    let device = DeviceId::new("local/cpu/candle")?;
    for size in [1usize, 8, 32] {
        let requests = (0..size)
            .map(|_| {
                AiRequest::text(
                    AiTask::ClassifyText,
                    "bounded candle batch input",
                    AiLimits::default(),
                )
            })
            .collect::<AiResult<Vec<_>>>()?;
        report(
            format,
            &format!("candle_infer_batch_{size}"),
            measure(COLD_ITERATIONS, || {
                std::hint::black_box(block_on(backend.infer_batch(
                    &requests,
                    &descriptor,
                    &device,
                    &CancellationToken::new(),
                ))?);
                Ok(())
            })?,
        );
    }
    Ok(())
}

#[cfg(feature = "training-candle")]
fn benchmark_candle_training(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let store: Arc<dyn ArtifactStore> = Arc::new(MemoryArtifactStore::new(4 << 20)?);
    let trainer = CandleTrainer::new(
        store,
        Arc::new(AllowTraining),
        CandleTrainerConfig::default(),
    )?;
    let examples = (0usize..64)
        .map(|index| TrainingExample {
            text: if index.is_multiple_of(2) {
                "alpha"
            } else {
                "beta"
            }
            .into(),
            label: index % 2,
        })
        .collect();
    let dataset: Arc<dyn TrainingDataset> =
        Arc::new(InMemoryTrainingDataset::new(examples, 64, 32)?);
    let job = TrainingJob {
        id: CapabilityId::new("perf/training")?,
        model: ModelId::new("perf/trained-linear")?,
        revision: "v1".into(),
        labels: vec!["alpha".into(), "beta".into()],
        input_dimensions: 64,
        epochs: 2,
        max_steps: 16,
        batch_size: 8,
        learning_rate: 0.05,
        seed: 7,
        resource_requirements: ResourceEstimate {
            cpu_percent: 25,
            memory_bytes: 1 << 20,
            workers: 1,
            ..ResourceEstimate::default()
        },
        resource_mode: AiResourceMode::Balanced,
        checkpoints: TrainingCheckpointPolicy {
            every_epochs: 0,
            max_checkpoints: 0,
        },
        resume: None,
        publisher: None,
        max_input_bytes: 32,
        max_output_bytes: 1_024,
    };
    report(
        format,
        "candle_training_64_examples_2_epochs",
        measure(16, || {
            std::hint::black_box(block_on(trainer.train(
                &job,
                Arc::clone(&dataset),
                Arc::new(IgnoreTrainingProgress),
                &CancellationToken::new(),
            ))?);
            Ok(())
        })?,
    );
    Ok(())
}

#[cfg(feature = "training-candle")]
#[derive(Debug)]
struct AllowTraining;

#[cfg(feature = "training-candle")]
impl TrainingAdmission for AllowTraining {
    fn admit(&self, _job: &TrainingJob) -> AiResult<AdmissionDecision> {
        Ok(AdmissionDecision::Admit {
            budget: ResourceBudget {
                cpu_percent: 100,
                gpu_percent: 0,
                memory_bytes: Some(4 << 20),
                vram_bytes: Some(0),
                storage_bytes: 4 << 20,
                workers: 1,
                concurrent_jobs: 1,
                pressure_limited: false,
            },
        })
    }
}

#[cfg(feature = "swarm")]
fn benchmark_swarm(format: &OutputFormat) -> Result<(), Box<dyn std::error::Error>> {
    let scheduler = CostScheduler::default();
    for count in [1usize, 10, 100, 1_000] {
        let candidates = remote_candidates(count)?;
        let iterations = FAST_ITERATIONS.min(20_000usize.saturating_div(count).max(20));
        report(
            format,
            &format!("swarm_scheduler_{count}_peers"),
            measure(iterations, || {
                std::hint::black_box(scheduler.plan(placement_context(true), &candidates));
                Ok(())
            })?,
        );
    }
    Ok(())
}

fn runtime_with_backends(models_count: usize, backend_count: usize) -> AiResult<AiRuntime> {
    let limits = AiLimits::default();
    let models = Arc::new(ModelRegistry::new());
    let backends = Arc::new(BackendRegistry::new());
    let ids = (0..backend_count)
        .map(|index| BackendId::new(format!("perf/backend-{index}")))
        .collect::<AiResult<Vec<_>>>()?;
    for id in &ids {
        backends.register(Arc::new(PerfBackend::new(id.clone())?))?;
    }
    for index in 0..models_count {
        models.register(model_descriptor(index, &ids)?, [ArtifactLocation::Memory])?;
    }
    AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000)?),
        models,
        backends,
        Arc::new(AllowAdmission),
    )
}

fn forced_request(model: &str) -> AiResult<AiRequest> {
    let mut request = AiRequest::text(AiTask::GenerateText, "bounded prompt", AiLimits::default())?;
    request.options.model = Some(ModelId::new(model)?);
    Ok(request)
}

fn model_descriptor(index: usize, backends: &[BackendId]) -> AiResult<ModelDescriptor> {
    let bytes = format!("perf-artifact-{index}");
    Ok(ModelDescriptor {
        id: ModelId::new(format!("perf/model-{index}"))?,
        revision: "v1".into(),
        tasks: vec![AiTask::GenerateText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::Gguf,
        quantization: Quantization::Int4,
        estimated_memory_bytes: 1_024,
        estimated_vram_bytes: 0,
        max_input_bytes: 1_024,
        max_output_bytes: 1_024,
        context_limit: Some(1_024),
        supported_backends: backends.to_vec(),
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 1,
        quality: Some(QualityTier::Tiny),
        artifact: ArtifactIdentity {
            digest: ArtifactDigest::from_bytes(bytes.as_bytes()),
            size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            publisher: None,
            signature_required: false,
        },
    })
}

fn placement_candidates(count: usize) -> AiResult<Vec<PlacementCandidate>> {
    (0..count)
        .map(|index| placement_candidate(index, false))
        .collect()
}

#[cfg(feature = "swarm")]
fn remote_candidates(count: usize) -> AiResult<Vec<PlacementCandidate>> {
    (0..count)
        .map(|index| placement_candidate(index, true))
        .collect()
}

fn placement_candidate(index: usize, remote: bool) -> AiResult<PlacementCandidate> {
    let target = if remote {
        ComputeTarget::RemotePeer {
            peer: PeerId::new(format!("perf/peer-{index}"))?,
            device: DeviceId::new(format!("perf/peer-{index}/cpu"))?,
            kind: DeviceKind::Cpu,
        }
    } else {
        ComputeTarget::LocalCpu(DeviceId::new(format!("perf/cpu-{index}"))?)
    };
    Ok(PlacementCandidate {
        key: PlacementKey {
            model: ModelId::new("perf/model")?,
            backend: BackendId::new(format!("perf/backend-{index}"))?,
            target,
        },
        health: BackendHealth::Healthy,
        resources: ResourceEstimate {
            memory_bytes: 1_024,
            workers: 1,
            ..ResourceEstimate::default()
        },
        metrics: PlacementMetrics {
            load_percent: Some(u8::try_from(index % 100).unwrap_or(100)),
            queue_depth: index % 4,
            available_memory_bytes: Some(1 << 20),
            available_vram_bytes: Some(0),
            latency_ema_ms: Some(u64::try_from(index % 50 + 1).unwrap_or(u64::MAX)),
            throughput_ema: Some(100),
        },
        model_resident: index.is_multiple_of(3),
        artifact_source: Some(ArtifactLocation::Memory),
        load_time_ms: 10,
        transfer_cost_units: 1,
        inference_cost_units: 1,
        rtt_ms: remote.then_some(u64::try_from(index % 100 + 1).unwrap_or(u64::MAX)),
        bandwidth_bytes_per_second: remote.then_some(1 << 20),
        trusted: true,
        failover_cost_units: 1,
    })
}

fn placement_context(remote: bool) -> PlacementContext {
    PlacementContext {
        priority: AiPriority::Normal,
        latency_class: AiLatencyClass::Balanced,
        resource_mode: AiResourceMode::Balanced,
        deadline_remaining: Some(Duration::from_secs(2)),
        allow_remote: remote,
        prefer_local: !remote,
        max_remote_latency: Duration::from_millis(250),
        pressure_limited: false,
    }
}

fn batch_key() -> AiResult<BatchKey> {
    Ok(BatchKey {
        model: ModelId::new("perf/model")?,
        backend: BackendId::new("perf/backend")?,
        device: DeviceId::new("perf/cpu")?,
        task: BatchTaskClass::GenerateText,
    })
}

#[derive(Debug)]
struct PerfBackend {
    descriptor: BackendDescriptor,
    loads: AtomicUsize,
}

impl PerfBackend {
    fn new(id: BackendId) -> AiResult<Self> {
        Ok(Self {
            descriptor: BackendDescriptor {
                id,
                tasks: vec![AiTask::GenerateText],
                input_modalities: vec![AiModality::Text],
                formats: vec![ArtifactFormat::Gguf],
                devices: vec![BackendDevice {
                    id: DeviceId::new("perf/cpu")?,
                    kind: DeviceKind::Cpu,
                }],
                costs: BackendCostHints {
                    load_units: 1,
                    inference_units: 1,
                    supports_batching: false,
                },
            },
            loads: AtomicUsize::new(0),
        })
    }
}

impl InferenceBackend for PerfBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn health(&self) -> BackendHealth {
        BackendHealth::Healthy
    }

    fn estimate(
        &self,
        _request: &AiRequest,
        _model: &ModelDescriptor,
        _device: &DeviceId,
    ) -> AiResult<ResourceEstimate> {
        Ok(ResourceEstimate {
            cpu_percent: 1,
            memory_bytes: 1_024,
            workers: 1,
            ..ResourceEstimate::default()
        })
    }

    fn load<'a>(
        &'a self,
        _model: &'a ModelDescriptor,
        _cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            self.loads.fetch_add(1, Ordering::Relaxed);
            Ok(())
        })
    }

    fn unload<'a>(
        &'a self,
        _model: &'a ModelDescriptor,
        _cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async { Ok(()) })
    }

    fn infer<'a>(
        &'a self,
        _request: &'a AiRequest,
        _model: &'a ModelDescriptor,
        _device: &'a DeviceId,
        _cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, AiResponse> {
        Box::pin(async {
            AiResponse::new(
                AiOutput::Text("ok".into()),
                Vec::new(),
                None,
                AiLimits::default(),
            )
        })
    }
}

#[derive(Debug)]
struct AllowAdmission;

impl ModelAdmission for AllowAdmission {
    fn admit(
        &self,
        _request: &AiRequest,
        _estimate: ResourceEstimate,
    ) -> AiResult<AdmissionDecision> {
        Ok(AdmissionDecision::Admit {
            budget: ResourceBudget {
                cpu_percent: 100,
                gpu_percent: 100,
                memory_bytes: Some(u64::MAX),
                vram_bytes: Some(u64::MAX),
                storage_bytes: u64::MAX,
                workers: 1,
                concurrent_jobs: 1,
                pressure_limited: false,
            },
        })
    }
}

#[derive(Clone, Copy)]
enum OutputFormat {
    Human,
    JsonLines,
}

impl OutputFormat {
    fn from_environment() -> Self {
        if std::env::var("APPCORE_AI_BENCH_FORMAT").as_deref() == Ok("jsonl") {
            Self::JsonLines
        } else {
            Self::Human
        }
    }
}

struct Measurement {
    iterations: usize,
    wall: Duration,
    p50: Duration,
    p95: Duration,
    p99: Duration,
}

fn measure(
    iterations: usize,
    mut operation: impl FnMut() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<Measurement, Box<dyn std::error::Error>> {
    let wall_started = Instant::now();
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        operation()?;
        samples.push(started.elapsed());
    }
    let wall = wall_started.elapsed();
    samples.sort_unstable();
    Ok(Measurement {
        iterations,
        wall,
        p50: percentile(&samples, 50),
        p95: percentile(&samples, 95),
        p99: percentile(&samples, 99),
    })
}

fn report(format: &OutputFormat, name: &str, measurement: Measurement) {
    let throughput = measurement.iterations as f64 / measurement.wall.as_secs_f64();
    match format {
        OutputFormat::Human => println!(
            "{name}: iterations={} ops/s={throughput:.0} wall={:?} p50={:?} p95={:?} p99={:?}",
            measurement.iterations,
            measurement.wall,
            measurement.p50,
            measurement.p95,
            measurement.p99,
        ),
        OutputFormat::JsonLines => println!(
            "{{\"benchmark\":\"{name}\",\"iterations\":{},\"throughput_ops_s\":{throughput:.3},\"wall_ns\":{},\"p50_ns\":{},\"p95_ns\":{},\"p99_ns\":{}}}",
            measurement.iterations,
            measurement.wall.as_nanos(),
            measurement.p50.as_nanos(),
            measurement.p95.as_nanos(),
            measurement.p99.as_nanos(),
        ),
    }
}

fn percentile(samples: &[Duration], percent: usize) -> Duration {
    let index = samples
        .len()
        .saturating_mul(percent)
        .div_ceil(100)
        .saturating_sub(1)
        .min(samples.len().saturating_sub(1));
    samples[index]
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let mut future = Box::pin(future);
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
