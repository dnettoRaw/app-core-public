// =============================================================================
//        #######
//     ###       ###     F: alpha_harness.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::*;
#[cfg(feature = "backend-openai-compatible")]
use std::collections::BTreeMap;
#[cfg(feature = "swarm")]
use std::collections::BTreeSet;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

const ITERATIONS: usize = 2_000;
const CONTENTION_ITERATIONS: usize = 128;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    report(
        "resolve_lightweight",
        measure(ITERATIONS, benchmark_resolve)?,
    );
    report(
        "scheduler_32_candidates",
        measure(ITERATIONS, benchmark_scheduler)?,
    );
    report("batch_1", measure(ITERATIONS, || benchmark_batching(1))?);
    report("batch_8", measure(ITERATIONS, || benchmark_batching(8))?);
    report(
        "model_registry_lookup",
        measure(ITERATIONS, benchmark_registry)?,
    );
    report(
        "residency_hot_reuse",
        measure(ITERATIONS, benchmark_residency)?,
    );
    report("resource_probe", measure(ITERATIONS, benchmark_probe)?);
    benchmark_artifact_cache()?;
    report(
        "fair_queue_contention_4_workers",
        measure(CONTENTION_ITERATIONS, benchmark_queue_contention)?,
    );
    #[cfg(feature = "swarm")]
    benchmark_swarm_planning()?;
    #[cfg(feature = "backend-openai-compatible")]
    benchmark_openai_compatible()?;
    #[cfg(feature = "backend-candle")]
    benchmark_candle()?;
    Ok(())
}

#[cfg(feature = "backend-openai-compatible")]
#[derive(Debug)]
struct BenchOpenAiTransport;

#[cfg(feature = "backend-openai-compatible")]
impl OpenAiCompatibleTransport for BenchOpenAiTransport {
    fn send<'a>(
        &'a self,
        _request: &'a OpenAiTransportRequest,
        _cancellation: &'a CancellationToken,
    ) -> OpenAiTransportFuture<'a> {
        Box::pin(async {
            Ok(OpenAiTransportResponse {
                status_code: 200,
                retry_after: None,
                body: br#"{"choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop"}]}"#
                    .to_vec(),
            })
        })
    }
}

#[cfg(feature = "backend-openai-compatible")]
fn benchmark_openai_compatible() -> Result<(), Box<dyn std::error::Error>> {
    let backend_id = BackendId::new("bench/openai-compatible")?;
    let model_id = ModelId::new("bench/chat")?;
    let device = DeviceId::new("bench/server")?;
    let mut names = BTreeMap::new();
    names.insert(model_id.clone(), "bench-chat".to_string());
    let backend = OpenAiCompatibleBackend::new(
        OpenAiCompatibleConfig::local(
            OpenAiCompatibleEngine::Generic,
            backend_id.clone(),
            "http://127.0.0.1:8080",
            vec![BackendDevice {
                id: device.clone(),
                kind: DeviceKind::Cpu,
            }],
            names,
        )?,
        Arc::new(BenchOpenAiTransport),
    )?;
    let descriptor = ModelDescriptor {
        id: model_id,
        revision: "v1".into(),
        tasks: vec![AiTask::Chat],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::Gguf,
        quantization: Quantization::Int4,
        estimated_memory_bytes: 1_024,
        estimated_vram_bytes: 0,
        max_input_bytes: 1_024,
        max_output_bytes: 1_024,
        context_limit: Some(1_024),
        supported_backends: vec![backend_id],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 1,
        quality: Some(QualityTier::Tiny),
        artifact: ArtifactIdentity {
            digest: ArtifactDigest::from_bytes(b"external-benchmark-model"),
            size_bytes: 24,
            publisher: None,
            signature_required: false,
        },
    };
    let request = AiRequest::chat(
        [AiMessage::new(AiMessageRole::User, "bounded prompt")?],
        AiLimits::default(),
    )?;
    report(
        "openai_compatible_adapter",
        measure(ITERATIONS, || {
            std::hint::black_box(block_on(backend.infer(
                &request,
                &descriptor,
                &device,
                &CancellationToken::new(),
            ))?);
            Ok(())
        })?,
    );
    Ok(())
}

fn benchmark_resolve() -> Result<(), Box<dyn std::error::Error>> {
    let limits = AiLimits::default();
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000)?),
        Arc::new(ModelRegistry::new()),
        Arc::new(BackendRegistry::new()),
        Arc::new(BenchAdmission),
    )?;
    let request = AiRequest::text(AiTask::TransformText, " bounded   text ", limits)?;
    block_on(runtime.resolve(request))?;
    Ok(())
}

fn benchmark_scheduler() -> Result<(), Box<dyn std::error::Error>> {
    let scheduler = CostScheduler::default();
    let mut candidates = Vec::with_capacity(32);
    for index in 0..32 {
        candidates.push(PlacementCandidate {
            key: PlacementKey {
                model: ModelId::new("bench/model")?,
                backend: BackendId::new(format!("bench/backend-{index}"))?,
                target: ComputeTarget::LocalCpu(DeviceId::new(format!("bench/cpu-{index}"))?),
            },
            health: BackendHealth::Healthy,
            resources: ResourceEstimate {
                memory_bytes: 1_024,
                workers: 1,
                ..ResourceEstimate::default()
            },
            metrics: PlacementMetrics {
                load_percent: Some(u8::try_from(index * 3).unwrap_or(100)),
                queue_depth: index % 4,
                available_memory_bytes: Some(1024 * 1024),
                available_vram_bytes: Some(0),
                latency_ema_ms: Some(u64::try_from(index + 1)?),
                throughput_ema: Some(100),
            },
            model_resident: index % 3 == 0,
            artifact_source: Some(ArtifactLocation::Memory),
            load_time_ms: 10,
            transfer_cost_units: 1,
            inference_cost_units: 1,
            rtt_ms: None,
            bandwidth_bytes_per_second: None,
            trusted: true,
            failover_cost_units: 1,
        });
    }
    let plan = scheduler.plan(
        PlacementContext {
            priority: AiPriority::Normal,
            latency_class: AiLatencyClass::Balanced,
            resource_mode: AiResourceMode::Balanced,
            deadline_remaining: Some(Duration::from_secs(1)),
            allow_remote: false,
            prefer_local: true,
            max_remote_latency: Duration::from_millis(250),
            pressure_limited: false,
        },
        &candidates,
    );
    std::hint::black_box(plan);
    Ok(())
}

fn benchmark_batching(size: usize) -> Result<(), Box<dyn std::error::Error>> {
    let mut batcher = DynamicBatcher::new(DynamicBatcherConfig {
        max_queues: 1,
        max_total_items: 16,
        max_queue_depth: 16,
        max_batch_size: size,
        max_wait: Duration::from_millis(1),
        overload_retry_after: Duration::from_millis(1),
    })?;
    let key = BatchKey {
        model: ModelId::new("bench/model")?,
        backend: BackendId::new("bench/backend")?,
        device: DeviceId::new("bench/cpu")?,
        task: BatchTaskClass::GenerateText,
    };
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
            available_memory_bytes: Some(1024 * 1024),
            available_vram_bytes: Some(0),
            estimated_item_memory_bytes: 1,
            estimated_item_vram_bytes: 0,
            device_load_percent: Some(0),
        },
        false,
    ));
    Ok(())
}

fn benchmark_artifact_cache() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = b"bounded benchmark artifact";
    let identity = ArtifactIdentity {
        digest: ArtifactDigest::from_bytes(bytes),
        size_bytes: u64::try_from(bytes.len())?,
        publisher: None,
        signature_required: false,
    };
    let missing = ArtifactIdentity {
        digest: ArtifactDigest::from_bytes(b"missing benchmark artifact"),
        size_bytes: 26,
        publisher: None,
        signature_required: false,
    };
    let store = MemoryArtifactStore::new(1024)?;
    store.store(&identity, bytes, &CancellationToken::new())?;
    report(
        "artifact_cache_hit",
        measure(ITERATIONS, || {
            std::hint::black_box(store.load(&identity, 1024, &CancellationToken::new())?);
            Ok(())
        })?,
    );
    report(
        "artifact_cache_miss",
        measure(ITERATIONS, || {
            let result = store.load(&missing, 1024, &CancellationToken::new());
            if !matches!(result, Err(AiError::NotFound("artifact"))) {
                return Err("unexpected artifact cache miss result".into());
            }
            Ok(())
        })?,
    );
    Ok(())
}

fn benchmark_queue_contention() -> Result<(), Box<dyn std::error::Error>> {
    let queue = Arc::new(Mutex::new(FairQueue::new(FairQueueConfig {
        capacity: 128,
        starvation_after: Duration::from_millis(10),
        overload_retry_after: Duration::from_millis(1),
    })?));
    let mut workers = Vec::new();
    for worker in 0..4usize {
        let queue = Arc::clone(&queue);
        workers.push(std::thread::spawn(move || -> AiResult<()> {
            for item in 0..32usize {
                let mut guard = queue.lock().map_err(|_| AiError::InternalState)?;
                std::hint::black_box(guard.enqueue(
                    worker.saturating_mul(32).saturating_add(item),
                    AiPriority::Normal,
                    u64::try_from(item).unwrap_or(u64::MAX),
                    Some(Duration::from_secs(1)),
                    CancellationToken::new(),
                ));
                std::hint::black_box(guard.dequeue(u64::try_from(item).unwrap_or(u64::MAX)));
            }
            Ok(())
        }));
    }
    for worker in workers {
        worker
            .join()
            .map_err(|_| std::io::Error::other("benchmark worker panic"))??;
    }
    Ok(())
}

#[cfg(feature = "swarm")]
fn benchmark_swarm_planning() -> Result<(), Box<dyn std::error::Error>> {
    let scheduler = CostScheduler::default();
    let candidates = simulated_remote_candidates(16)?;
    report(
        "swarm_scheduler_16_peers",
        measure(ITERATIONS, || {
            std::hint::black_box(scheduler.plan(remote_context(), &candidates));
            Ok(())
        })?,
    );
    report(
        "swarm_failover_replan_16_peers",
        measure(ITERATIONS, || {
            let mut retry = candidates.clone();
            retry[0].health = BackendHealth::Unavailable;
            std::hint::black_box(scheduler.plan(remote_context(), &retry));
            Ok(())
        })?,
    );
    benchmark_peer_directory()?;
    Ok(())
}

#[cfg(feature = "swarm")]
fn simulated_remote_candidates(count: usize) -> AiResult<Vec<PlacementCandidate>> {
    (0..count)
        .map(|index| {
            Ok(PlacementCandidate {
                key: PlacementKey {
                    model: ModelId::new("bench/swarm-model")?,
                    backend: BackendId::new(format!("bench/remote-{index}"))?,
                    target: ComputeTarget::RemotePeer {
                        peer: PeerId::new(format!("bench/peer-{index}"))?,
                        device: DeviceId::new(format!("bench/peer-{index}/cpu"))?,
                        kind: DeviceKind::Cpu,
                    },
                },
                health: BackendHealth::Healthy,
                resources: ResourceEstimate {
                    cpu_percent: 20,
                    memory_bytes: 1_024,
                    workers: 1,
                    ..ResourceEstimate::default()
                },
                metrics: PlacementMetrics {
                    load_percent: Some(u8::try_from(index.saturating_mul(3)).unwrap_or(100)),
                    queue_depth: index % 4,
                    available_memory_bytes: Some(1024 * 1024),
                    available_vram_bytes: Some(0),
                    latency_ema_ms: Some(
                        u64::try_from(index.saturating_add(5)).unwrap_or(u64::MAX),
                    ),
                    throughput_ema: Some(100),
                },
                model_resident: index % 3 == 0,
                artifact_source: Some(ArtifactLocation::Peer(PeerId::new(format!(
                    "bench/storage-{index}"
                ))?)),
                load_time_ms: 10,
                transfer_cost_units: u64::try_from(index.saturating_add(1)).unwrap_or(u64::MAX),
                inference_cost_units: 1,
                rtt_ms: Some(u64::try_from(index.saturating_add(1)).unwrap_or(u64::MAX)),
                bandwidth_bytes_per_second: Some(1024 * 1024),
                trusted: true,
                failover_cost_units: 2,
            })
        })
        .collect()
}

#[cfg(feature = "swarm")]
fn remote_context() -> PlacementContext {
    PlacementContext {
        priority: AiPriority::Normal,
        latency_class: AiLatencyClass::Balanced,
        resource_mode: AiResourceMode::Balanced,
        deadline_remaining: Some(Duration::from_secs(1)),
        allow_remote: true,
        prefer_local: true,
        max_remote_latency: Duration::from_millis(250),
        pressure_limited: false,
    }
}

#[cfg(feature = "swarm")]
fn benchmark_peer_directory() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PeerCapabilityDirectory::new(PeerDirectoryConfig {
        max_peers: 16,
        max_devices_per_peer: 1,
        max_artifacts_per_peer: 1,
        max_ttl: Duration::from_secs(5),
    })?;
    let authorizer = BenchPeerAuthorizer;
    let tenant = CapabilityId::new("bench/tenant")?;
    let mut advertisements = Vec::new();
    for index in 0..16usize {
        let capabilities = AiNodeCapabilities::from_contribution(
            PeerId::new(format!("bench/peer-{index}"))?,
            tenant.clone(),
            0,
            1_000,
            vec![AdvertisedCompute {
                backend: BackendId::new("bench/remote")?,
                device: DeviceId::new(format!("bench/peer-{index}/cpu"))?,
                kind: DeviceKind::Cpu,
                metrics: PlacementMetrics::default(),
            }],
            None,
            BTreeSet::new(),
            ResourceBudget {
                cpu_percent: 20,
                gpu_percent: 0,
                memory_bytes: Some(1024),
                vram_bytes: Some(0),
                storage_bytes: 0,
                workers: 1,
                concurrent_jobs: 1,
                pressure_limited: false,
            },
            AiContributionPolicy {
                contribute_compute: true,
                contribute_storage: false,
                max_cpu_percent: 20,
                max_gpu_percent: 0,
                max_memory_bytes: 1024,
                max_vram_bytes: 0,
                max_storage_bytes: 0,
                max_workers: 1,
                max_concurrent_jobs: 1,
            },
        )?;
        directory.update(capabilities.clone(), &authorizer, 0)?;
        advertisements.push(capabilities);
    }
    report(
        "swarm_discovery_16_peers",
        measure(ITERATIONS, || {
            std::hint::black_box(directory.live(&tenant, 1, 16)?);
            Ok(())
        })?,
    );
    let mut update_expiry = 1_000u64;
    report(
        "swarm_advertisement_update",
        measure(ITERATIONS, || {
            update_expiry = update_expiry.saturating_add(1);
            let mut update = advertisements[0].clone();
            update.expires_at_ms = update_expiry;
            directory.update(update, &authorizer, 0)?;
            Ok(())
        })?,
    );
    Ok(())
}

#[cfg(feature = "swarm")]
#[derive(Debug)]
struct BenchPeerAuthorizer;

#[cfg(feature = "swarm")]
impl PeerCapabilityAuthorizer for BenchPeerAuthorizer {
    fn authorize(&self, capabilities: &AiNodeCapabilities) -> AiResult<PeerAuthorization> {
        Ok(PeerAuthorization {
            authenticated: true,
            tenants: [capabilities.tenant.clone()].into_iter().collect(),
            allow_compute: true,
            allow_storage: false,
        })
    }
}

#[cfg(feature = "backend-candle")]
fn benchmark_candle() -> Result<(), Box<dyn std::error::Error>> {
    use candle_core::{Device, Tensor};

    let dimensions = 64usize;
    let direct_weights =
        Tensor::from_vec(vec![0.01f32; dimensions * 2], (2, dimensions), &Device::Cpu)?;
    let direct_biases = Tensor::from_vec(vec![0.0f32; 2], 2, &Device::Cpu)?;
    report(
        "candle_direct_linear",
        measure(ITERATIONS, || {
            let input = Tensor::from_vec(vec![0.01f32; dimensions], (1, dimensions), &Device::Cpu)?;
            let output = input
                .matmul(&direct_weights.t()?)?
                .broadcast_add(&direct_biases)?;
            std::hint::black_box(candle_nn::ops::softmax_last_dim(&output)?.to_vec2::<f32>()?);
            Ok(())
        })?,
    );

    let artifact = NativeLinearArtifact::new(
        dimensions,
        vec!["a".into(), "b".into()],
        vec![0.01; dimensions * 2],
        vec![0.0; 2],
    )?;
    let bytes = artifact.encode()?;
    let identity = artifact.identity(None, false)?;
    let memory = Arc::new(MemoryArtifactStore::new(1024 * 1024)?);
    memory.store(&identity, &bytes, &CancellationToken::new())?;
    let store: Arc<dyn ArtifactStore> = memory;
    let descriptor = ModelDescriptor {
        id: ModelId::new("bench/candle")?,
        revision: "v1".into(),
        tasks: vec![AiTask::ClassifyText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::NativeLinearV1,
        quantization: Quantization::None,
        estimated_memory_bytes: 1024,
        estimated_vram_bytes: 0,
        max_input_bytes: 1024,
        max_output_bytes: 1024,
        context_limit: None,
        supported_backends: vec![BackendId::new(CANDLE_LINEAR_BACKEND_ID)?],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 1,
        quality: Some(QualityTier::Tiny),
        artifact: identity,
    };
    report(
        "candle_backend_startup",
        measure(CONTENTION_ITERATIONS, || {
            std::hint::black_box(CandleBackend::new(
                Arc::clone(&store),
                CandleBackendConfig::default(),
            )?);
            Ok(())
        })?,
    );
    report(
        "candle_model_load",
        measure(CONTENTION_ITERATIONS, || {
            let backend = CandleBackend::new(Arc::clone(&store), CandleBackendConfig::default())?;
            block_on(backend.load(&descriptor, &CancellationToken::new()))?;
            Ok(())
        })?,
    );
    let backend = CandleBackend::new(store, CandleBackendConfig::default())?;
    block_on(backend.load(&descriptor, &CancellationToken::new()))?;
    let request = AiRequest::text(AiTask::ClassifyText, "a", AiLimits::default())?;
    let device = DeviceId::new("local/cpu/candle")?;
    report(
        "appcore_candle_adapter",
        measure(ITERATIONS, || {
            std::hint::black_box(block_on(backend.infer(
                &request,
                &descriptor,
                &device,
                &CancellationToken::new(),
            ))?);
            Ok(())
        })?,
    );
    Ok(())
}

fn benchmark_registry() -> Result<(), Box<dyn std::error::Error>> {
    let registry = ModelRegistry::new();
    let descriptor = descriptor("bench/registry", b"registry")?;
    let id = descriptor.id.clone();
    registry.register(descriptor, [ArtifactLocation::Memory])?;
    std::hint::black_box(registry.get(&id)?);
    Ok(())
}

fn benchmark_residency() -> Result<(), Box<dyn std::error::Error>> {
    let planner = ResidencyPlanner::new(
        ResidencyConfig::default(),
        vec![TierCapacity {
            tier: ResidencyTier::Memory,
            capacity_bytes: 1024 * 1024,
        }],
    )?;
    planner.register(ResidencyRecord {
        model: ModelId::new("bench/resident")?,
        tier: ResidencyTier::Memory,
        size_bytes: 1_024,
        last_used_ms: 0,
        use_count: 1,
        load_time_ms: 1,
        importance_basis_points: 1_000,
        predicted_next_use_ms: None,
    })?;
    std::hint::black_box(planner.begin(ResidencyRequest {
        model: ModelId::new("bench/resident")?,
        preferred: ResidencyTier::Memory,
        fallbacks: Vec::new(),
        size_bytes: 1_024,
        load_time_ms: 1,
        importance_basis_points: 1_000,
        predicted_next_use_ms: Some(2),
        now_ms: 1,
        resource_mode: AiResourceMode::Balanced,
        capacity_limit_bytes: None,
        prefetch: false,
        cancellation: CancellationToken::new(),
    })?);
    Ok(())
}

fn benchmark_probe() -> Result<(), Box<dyn std::error::Error>> {
    std::hint::black_box(SystemHardwareProbe::default().sample()?);
    Ok(())
}

fn descriptor(id: &str, bytes: &[u8]) -> AiResult<ModelDescriptor> {
    Ok(ModelDescriptor {
        id: ModelId::new(id)?,
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
        supported_backends: vec![BackendId::new("bench/backend")?],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 1,
        quality: Some(QualityTier::Tiny),
        artifact: ArtifactIdentity {
            digest: ArtifactDigest::from_bytes(bytes),
            size_bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
            publisher: None,
            signature_required: false,
        },
    })
}

fn measure(
    iterations: usize,
    mut operation: impl FnMut() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<Vec<Duration>, Box<dyn std::error::Error>> {
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        operation()?;
        samples.push(started.elapsed());
    }
    Ok(samples)
}

fn report(name: &str, mut samples: Vec<Duration>) {
    samples.sort_unstable();
    let p50 = percentile(&samples, 50);
    let p95 = percentile(&samples, 95);
    let p99 = percentile(&samples, 99);
    let total = samples.iter().copied().sum::<Duration>();
    let throughput = samples.len() as f64 / total.as_secs_f64();
    println!("{name}: ops/s={throughput:.0} p50={p50:?} p95={p95:?} p99={p99:?}");
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

#[derive(Debug)]
struct BenchAdmission;

impl ModelAdmission for BenchAdmission {
    fn admit(
        &self,
        _request: &AiRequest,
        _estimate: ResourceEstimate,
    ) -> AiResult<AdmissionDecision> {
        Err(AiError::Capacity("benchmark backend path is unused"))
    }
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
