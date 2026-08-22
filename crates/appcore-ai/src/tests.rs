// =============================================================================
//        #######
//     ###       ###     F: tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::*;
#[cfg(feature = "swarm")]
use std::collections::BTreeSet;
use std::collections::{BTreeMap, VecDeque};
use std::future::Future;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::time::Duration;

#[derive(Debug)]
struct FakeProbe {
    samples: Mutex<VecDeque<ResourceSnapshot>>,
}

impl FakeProbe {
    fn new(samples: Vec<ResourceSnapshot>) -> Self {
        Self {
            samples: Mutex::new(samples.into()),
        }
    }
}

impl HardwareProbe for FakeProbe {
    fn sample(&self) -> AiResult<ResourceSnapshot> {
        self.samples
            .lock()
            .map_err(|_| AiError::InternalState)?
            .pop_front()
            .ok_or(AiError::NotFound("fake resource sample"))
    }
}

fn snapshot(cpu_load_percent: Option<u8>) -> ResourceSnapshot {
    ResourceSnapshot {
        logical_cpus: Some(8),
        physical_cpus: Some(4),
        cpu_load_percent,
        process_cpu_percent: Some(1),
        total_memory_bytes: Some(16 * 1024 * 1024 * 1024),
        available_memory_bytes: Some(12 * 1024 * 1024 * 1024),
        used_memory_bytes: Some(4 * 1024 * 1024 * 1024),
        memory_pressure_percent: Some(0),
        devices: vec![DeviceSnapshot {
            id: DeviceId::new("local/gpu/0").unwrap(),
            kind: DeviceKind::Gpu,
            capabilities: DeviceCapabilities {
                class: DeviceClass::DiscreteGpu,
                memory_kind: DeviceMemoryKind::Dedicated,
                compatible_apis: Vec::new(),
            },
            total_memory_bytes: Some(8 * 1024 * 1024 * 1024),
            available_memory_bytes: Some(6 * 1024 * 1024 * 1024),
            used_memory_bytes: Some(2 * 1024 * 1024 * 1024),
            utilization_percent: Some(10),
            healthy: true,
        }],
        queue_depth: 0,
        active_jobs: 0,
        thermal_pressure: ThermalPressure::Nominal,
        status: ResourceProbeStatus::Healthy,
        ..ResourceSnapshot::default()
    }
}

fn governor_config() -> ResourceGovernorConfig {
    ResourceGovernorConfig {
        sampling_interval: Duration::from_millis(1),
        hysteresis_samples: 2,
        max_workers: 16,
        max_concurrent_jobs: 8,
        reserved_memory_bytes: 0,
        pressure_queue_depth: 16,
    }
}

fn model_descriptor(id: &str, bytes: &[u8]) -> ModelDescriptor {
    ModelDescriptor {
        id: ModelId::new(id).unwrap(),
        revision: "v1".into(),
        tasks: vec![AiTask::ClassifyText, AiTask::GenerateText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::NativeLinearV1,
        quantization: Quantization::None,
        estimated_memory_bytes: 1024,
        estimated_vram_bytes: 0,
        max_input_bytes: 1024,
        max_output_bytes: 1024,
        context_limit: None,
        supported_backends: vec![BackendId::new("native").unwrap()],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 5,
        quality: Some(QualityTier::Tiny),
        artifact: ArtifactIdentity {
            digest: ArtifactDigest::from_bytes(bytes),
            size_bytes: u64::try_from(bytes.len()).unwrap(),
            publisher: None,
            signature_required: false,
        },
    }
}

fn temporary_directory(name: &str) -> std::path::PathBuf {
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!("appcore-ai-{name}-{}-{nonce}", std::process::id()))
}

#[derive(Debug)]
struct FakeBackend {
    descriptor: BackendDescriptor,
    fail_inference: bool,
    loaded: AtomicBool,
    load_count: AtomicUsize,
    inference_count: AtomicUsize,
    load_delay: Duration,
}

impl FakeBackend {
    fn new(id: &str, fail_inference: bool) -> Self {
        Self {
            descriptor: BackendDescriptor {
                id: BackendId::new(id).unwrap(),
                tasks: vec![AiTask::GenerateText, AiTask::ClassifyText],
                input_modalities: vec![AiModality::Text],
                formats: vec![ArtifactFormat::NativeLinearV1],
                devices: vec![BackendDevice {
                    id: DeviceId::new("local/cpu").unwrap(),
                    kind: DeviceKind::Cpu,
                }],
                costs: BackendCostHints {
                    load_units: 1,
                    inference_units: 1,
                    supports_batching: true,
                },
            },
            fail_inference,
            loaded: AtomicBool::new(false),
            load_count: AtomicUsize::new(0),
            inference_count: AtomicUsize::new(0),
            load_delay: Duration::ZERO,
        }
    }

    fn with_load_delay(mut self, load_delay: Duration) -> Self {
        self.load_delay = load_delay;
        self
    }
}

impl InferenceBackend for FakeBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn health(&self) -> BackendHealth {
        BackendHealth::Healthy
    }

    fn estimate(
        &self,
        _request: &AiRequest,
        model: &ModelDescriptor,
        _device: &DeviceId,
    ) -> AiResult<ResourceEstimate> {
        Ok(ResourceEstimate {
            cpu_percent: 10,
            memory_bytes: model.estimated_memory_bytes,
            vram_bytes: model.estimated_vram_bytes,
            workers: 1,
            ..ResourceEstimate::default()
        })
    }

    fn load<'a>(
        &'a self,
        _model: &'a ModelDescriptor,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                Err(AiError::Cancelled)
            } else {
                self.load_count.fetch_add(1, Ordering::Relaxed);
                if !self.load_delay.is_zero() {
                    std::thread::sleep(self.load_delay);
                }
                self.loaded.store(true, Ordering::Release);
                Ok(())
            }
        })
    }

    fn unload<'a>(
        &'a self,
        _model: &'a ModelDescriptor,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(AiError::Cancelled);
            }
            self.loaded.store(false, Ordering::Release);
            Ok(())
        })
    }

    fn infer<'a>(
        &'a self,
        _request: &'a AiRequest,
        _model: &'a ModelDescriptor,
        _device: &'a DeviceId,
        _cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, AiResponse> {
        Box::pin(async move {
            self.inference_count.fetch_add(1, Ordering::Relaxed);
            if !self.loaded.load(Ordering::Acquire) {
                return Err(AiError::BackendUnavailable(self.descriptor.id.clone()));
            }
            if self.fail_inference {
                return Err(AiError::BackendFailure {
                    backend: self.descriptor.id.clone(),
                    code: "simulated",
                });
            }
            AiResponse::new(
                AiOutput::Text("backend-answer".into()),
                Vec::new(),
                None,
                AiLimits::default(),
            )
        })
    }
}

#[derive(Debug)]
struct AlwaysAdmit;

impl ModelAdmission for AlwaysAdmit {
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
                storage_bytes: 0,
                workers: 64,
                concurrent_jobs: 64,
                pressure_limited: false,
            },
        })
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

#[test]
fn identifiers_are_bounded_and_reject_unsafe_characters() {
    assert!(ModelId::new("vendor/model:v1").is_ok());
    assert_eq!(
        ModelId::new("../model").unwrap_err(),
        AiError::InvalidInput("identifier characters")
    );
    assert!(BackendId::new("x".repeat(97)).is_err());
}

#[test]
fn request_validation_enforces_input_and_privacy_limits() {
    let limits = AiLimits {
        max_input_bytes: 4,
        ..AiLimits::default()
    };
    assert!(AiRequest::text(AiTask::TransformText, "1234", limits).is_ok());
    assert!(matches!(
        AiRequest::text(AiTask::TransformText, "12345", limits),
        Err(AiError::LimitExceeded {
            kind: LimitKind::InputBytes,
            ..
        })
    ));

    let mut request = AiRequest::text(AiTask::Decide, "ready", AiLimits::default()).unwrap();
    request.options.privacy = AiPrivacyMode::LocalOnly;
    request.options.execution = AiExecutionMode::Swarm;
    assert_eq!(
        request.validate(AiLimits::default()),
        Err(AiError::InvalidInput(
            "local-only request permits remote resources"
        ))
    );

    request.options.execution = AiExecutionMode::Local;
    request.options.distribution.allow_remote_storage = true;
    assert_eq!(
        request.validate(AiLimits::default()),
        Err(AiError::InvalidInput(
            "local-only request permits remote resources"
        ))
    );
}

#[test]
fn multimodal_requests_require_the_declared_image_or_document_part() {
    let limits = AiLimits::default();
    let image_input = AiInput::new(
        vec![
            AiContent::Text("describe".into()),
            AiContent::Binary {
                media_type: "image/png".into(),
                bytes: vec![1, 2, 3],
            },
        ],
        limits,
    )
    .unwrap();
    assert_eq!(
        image_input.modalities(),
        vec![AiModality::Text, AiModality::Image]
    );
    let image = AiRequest {
        task: AiTask::AnalyzeImage,
        input: image_input,
        options: AiOptions::default(),
    };
    assert!(image.validate(limits).is_ok());

    let text_only = AiRequest::text(AiTask::AnalyzeImage, "describe", limits).unwrap();
    assert_eq!(
        text_only.validate(limits),
        Err(AiError::InvalidInput(
            "image analysis requires image content"
        ))
    );

    let document = AiRequest {
        task: AiTask::AnalyzeDocument,
        input: AiInput::new(
            vec![AiContent::Binary {
                media_type: "application/pdf".into(),
                bytes: b"%PDF".to_vec(),
            }],
            limits,
        )
        .unwrap(),
        options: AiOptions::default(),
    };
    assert!(document.validate(limits).is_ok());
}

#[test]
fn quality_target_filters_automatic_models_but_honors_a_forced_model() {
    let models = ModelRegistry::new();
    let mut tiny = model_descriptor("model/tiny", b"tiny");
    tiny.quality = Some(QualityTier::Tiny);
    let tiny_id = tiny.id.clone();
    models
        .register(tiny, [ArtifactLocation::LocalStorage])
        .unwrap();
    let mut deep = model_descriptor("model/deep", b"deep");
    deep.quality = Some(QualityTier::Balanced);
    models
        .register(deep, [ArtifactLocation::LocalStorage])
        .unwrap();

    let mut request = AiRequest::text(
        AiTask::GenerateText,
        "complex question",
        AiLimits::default(),
    )
    .unwrap();
    request.options.quality = AiQualityTarget::Deep;
    let candidates = crate::router_support::model_candidates(&models, &request).unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].descriptor.quality,
        Some(QualityTier::Balanced)
    );

    request.options.quality = AiQualityTarget::Maximum;
    assert!(crate::router_support::model_candidates(&models, &request)
        .unwrap()
        .is_empty());
    request.options.model = Some(tiny_id);
    assert_eq!(
        crate::router_support::model_candidates(&models, &request)
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn response_metadata_and_attempts_are_bounded() {
    let limits = AiLimits {
        max_metadata_entries: 1,
        ..AiLimits::default()
    };
    let metadata = vec![
        AiMetadata {
            key: "route".into(),
            value: "local".into(),
        },
        AiMetadata {
            key: "model".into(),
            value: "small".into(),
        },
    ];
    assert!(matches!(
        AiResponse::new(AiOutput::Text("ok".into()), metadata, None, limits),
        Err(AiError::LimitExceeded {
            kind: LimitKind::MetadataEntries,
            ..
        })
    ));
}

#[test]
fn cancellation_is_shared_between_clones() {
    let token = CancellationToken::new();
    let clone = token.clone();
    clone.cancel();
    assert!(token.is_cancelled());
}

#[test]
fn local_and_contribution_budgets_are_distinct() {
    let contribution = AiContributionPolicy {
        contribute_compute: true,
        contribute_storage: true,
        max_cpu_percent: 25,
        max_gpu_percent: 20,
        max_memory_bytes: 1024 * 1024 * 1024,
        max_vram_bytes: 512 * 1024 * 1024,
        max_storage_bytes: 20 * 1024 * 1024 * 1024,
        max_workers: 2,
        max_concurrent_jobs: 1,
    };
    let governor = ResourceGovernor::new(
        FakeProbe::new(vec![snapshot(Some(10))]),
        governor_config(),
        contribution,
    )
    .unwrap();
    let budgets = governor.budgets(AiResourceMode::Unrestricted, 0).unwrap();

    assert_eq!(budgets.local.cpu_percent, 100);
    assert_eq!(budgets.contribution.cpu_percent, 25);
    assert_eq!(budgets.contribution.workers, 2);
    assert_eq!(budgets.contribution.concurrent_jobs, 1);
    assert_eq!(budgets.contribution.storage_bytes, 20 * 1024 * 1024 * 1024);
    assert!(budgets.contribution.memory_bytes <= Some(1024 * 1024 * 1024));
}

#[test]
fn pressure_uses_hysteresis_before_reducing_budget() {
    let governor = ResourceGovernor::new(
        FakeProbe::new(vec![snapshot(Some(99)), snapshot(Some(99))]),
        governor_config(),
        AiContributionPolicy::default(),
    )
    .unwrap();

    let first = governor.budgets(AiResourceMode::Performance, 0).unwrap();
    let second = governor.budgets(AiResourceMode::Performance, 2).unwrap();
    assert!(!first.local.pressure_limited);
    assert!(second.local.pressure_limited);
    assert!(second.local.workers < first.local.workers);
}

#[test]
fn concurrent_governor_calls_share_one_probe_sample_without_holding_state_lock() {
    #[derive(Debug)]
    struct CountingProbe {
        calls: Arc<AtomicUsize>,
        value: ResourceSnapshot,
    }

    impl HardwareProbe for CountingProbe {
        fn sample(&self) -> AiResult<ResourceSnapshot> {
            self.calls.fetch_add(1, Ordering::Relaxed);
            std::thread::sleep(Duration::from_millis(10));
            Ok(self.value.clone())
        }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let mut config = governor_config();
    config.sampling_interval = Duration::from_secs(1);
    let governor = Arc::new(
        ResourceGovernor::new(
            CountingProbe {
                calls: Arc::clone(&calls),
                value: snapshot(Some(10)),
            },
            config,
            AiContributionPolicy::default(),
        )
        .unwrap(),
    );
    let barrier = Arc::new(std::sync::Barrier::new(17));
    let mut callers = Vec::new();
    for _ in 0..16 {
        let governor = Arc::clone(&governor);
        let barrier = Arc::clone(&barrier);
        callers.push(std::thread::spawn(move || {
            barrier.wait();
            governor.budgets(AiResourceMode::Balanced, 0)
        }));
    }
    barrier.wait();
    for caller in callers {
        assert!(caller.join().unwrap().is_ok());
    }
    assert_eq!(calls.load(Ordering::Relaxed), 1);
}

#[test]
fn unknown_memory_is_deferred_instead_of_assumed_unlimited() {
    let mut unknown = snapshot(Some(10));
    unknown.total_memory_bytes = None;
    unknown.available_memory_bytes = None;
    let governor = ResourceGovernor::new(
        FakeProbe::new(vec![unknown]),
        governor_config(),
        AiContributionPolicy::default(),
    )
    .unwrap();
    let decision = governor
        .admit(
            AiResourceMode::Balanced,
            ResourceEstimate {
                cpu_percent: 10,
                memory_bytes: 1,
                workers: 1,
                ..ResourceEstimate::default()
            },
            0,
        )
        .unwrap();

    assert!(matches!(
        decision,
        AdmissionDecision::Defer {
            reason: AdmissionReason::UnknownCapacity,
            ..
        }
    ));
}

#[test]
fn system_probe_reports_real_capacity_or_explicitly_marks_unsupported() {
    let snapshot = SystemHardwareProbe::default().sample().unwrap();
    assert!(snapshot.logical_cpus.is_some());
    if cfg!(any(target_os = "linux", target_os = "macos", windows)) {
        let total = snapshot.total_memory_bytes.unwrap();
        let available = snapshot.available_memory_bytes.unwrap();
        assert!(total > 0);
        assert!(available <= total);
        assert_ne!(snapshot.status, ResourceProbeStatus::Unsupported);
    } else {
        assert_eq!(snapshot.status, ResourceProbeStatus::Unsupported);
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    assert!(snapshot.devices.iter().any(|device| {
        device.kind == DeviceKind::Gpu
            && device.capabilities.memory_kind == DeviceMemoryKind::Unified
            && device.available_memory_bytes == snapshot.available_memory_bytes
    }));
}

#[test]
fn exact_multi_gpu_admission_never_aggregates_vram() {
    let mut value = snapshot(Some(10));
    value.devices = vec![
        dedicated_gpu("local/gpu/small", 16, 2, 10, true),
        dedicated_gpu("local/gpu/large", 24, 20, 20, true),
    ];
    let governor = ResourceGovernor::new(
        FakeProbe::new(vec![value]),
        governor_config(),
        AiContributionPolicy::default(),
    )
    .unwrap();
    let estimate = ResourceEstimate {
        gpu_percent: 50,
        memory_bytes: 1,
        vram_bytes: 12 * 1024 * 1024 * 1024,
        workers: 1,
        ..ResourceEstimate::default()
    };
    let small = governor
        .admit_on(
            AiResourceMode::Unrestricted,
            estimate,
            DeviceKind::Gpu,
            &DeviceId::new("local/gpu/small").unwrap(),
            0,
        )
        .unwrap();
    let large = governor
        .admit_on(
            AiResourceMode::Unrestricted,
            estimate,
            DeviceKind::Gpu,
            &DeviceId::new("local/gpu/large").unwrap(),
            0,
        )
        .unwrap();
    assert!(matches!(
        small,
        AdmissionDecision::Defer {
            reason: AdmissionReason::VramPressure,
            ..
        }
    ));
    assert!(matches!(large, AdmissionDecision::Admit { .. }));
}

#[test]
fn unified_gpu_uses_one_memory_pool_and_one_residency_tier() {
    let mut value = snapshot(Some(10));
    value.total_memory_bytes = Some(16 * 1024 * 1024 * 1024);
    value.available_memory_bytes = Some(6 * 1024 * 1024 * 1024);
    value.devices = vec![unified_gpu("local/gpu/unified", 16, 6, true)];
    let governor = ResourceGovernor::new(
        FakeProbe::new(vec![value]),
        governor_config(),
        AiContributionPolicy::default(),
    )
    .unwrap();
    let decision = governor
        .admit_on(
            AiResourceMode::Unrestricted,
            ResourceEstimate {
                gpu_percent: 50,
                memory_bytes: 4 * 1024 * 1024 * 1024,
                vram_bytes: 4 * 1024 * 1024 * 1024,
                workers: 1,
                ..ResourceEstimate::default()
            },
            DeviceKind::Gpu,
            &DeviceId::new("local/gpu/unified").unwrap(),
            0,
        )
        .unwrap();
    assert!(matches!(
        decision,
        AdmissionDecision::Defer {
            reason: AdmissionReason::MemoryPressure,
            ..
        }
    ));
    let capacities = governor
        .residency_capacities(AiResourceMode::Unrestricted, 0)
        .unwrap();
    assert_eq!(capacities.len(), 1);
    assert_eq!(capacities[0].tier, ResidencyTier::Memory);
}

#[test]
fn disappeared_gpu_stops_new_placement_without_corrupting_governor() {
    let mut present = snapshot(Some(10));
    present.devices = vec![dedicated_gpu("local/gpu/reset", 16, 12, 10, true)];
    let mut disappeared = present.clone();
    disappeared.devices[0].healthy = false;
    let governor = ResourceGovernor::new(
        FakeProbe::new(vec![present, disappeared]),
        governor_config(),
        AiContributionPolicy::default(),
    )
    .unwrap();
    let estimate = ResourceEstimate {
        gpu_percent: 20,
        memory_bytes: 1,
        vram_bytes: 1024,
        workers: 1,
        ..ResourceEstimate::default()
    };
    assert!(matches!(
        governor
            .admit_on(
                AiResourceMode::Unrestricted,
                estimate,
                DeviceKind::Gpu,
                &DeviceId::new("local/gpu/reset").unwrap(),
                0,
            )
            .unwrap(),
        AdmissionDecision::Admit { .. }
    ));
    assert!(matches!(
        governor
            .admit_on(
                AiResourceMode::Unrestricted,
                estimate,
                DeviceKind::Gpu,
                &DeviceId::new("local/gpu/reset").unwrap(),
                2,
            )
            .unwrap(),
        AdmissionDecision::Defer {
            reason: AdmissionReason::DeviceUnavailable,
            ..
        }
    ));
    assert_eq!(governor.metrics(2).unwrap().admission_denied, 1);
}

#[test]
fn hardware_sampler_is_single_flight_and_reports_snapshot_age() {
    #[derive(Debug)]
    struct SlowProbe {
        calls: Arc<AtomicUsize>,
    }
    impl HardwareProbe for SlowProbe {
        fn sample(&self) -> AiResult<ResourceSnapshot> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(10));
            Ok(snapshot(Some(10)))
        }
    }
    let calls = Arc::new(AtomicUsize::new(0));
    let sampler = Arc::new(
        HardwareSampler::new(
            SlowProbe {
                calls: Arc::clone(&calls),
            },
            Duration::from_secs(1),
        )
        .unwrap(),
    );
    let threads = (0..100)
        .map(|_| {
            let sampler = Arc::clone(&sampler);
            std::thread::spawn(move || sampler.sample().unwrap())
        })
        .collect::<Vec<_>>();
    for thread in threads {
        let sampled = thread.join().unwrap();
        assert!(sampled.captured_at_unix_ms.is_some());
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let metrics = sampler.metrics();
    assert_eq!(metrics.samples, 1);
    assert_eq!(metrics.cache_hits, 99);
    assert!(metrics.snapshot_age.is_some());
}

#[test]
fn concurrent_sampler_failures_are_single_flight_and_throttled() {
    #[derive(Debug)]
    struct SlowFailProbe(Arc<AtomicUsize>);

    impl HardwareProbe for SlowFailProbe {
        fn sample(&self) -> AiResult<ResourceSnapshot> {
            self.0.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(10));
            Err(AiError::Capacity("hardware sample"))
        }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let sampler = Arc::new(
        HardwareSampler::new(SlowFailProbe(Arc::clone(&calls)), Duration::from_secs(1)).unwrap(),
    );
    let barrier = Arc::new(std::sync::Barrier::new(33));
    let threads = (0..32)
        .map(|_| {
            let sampler = Arc::clone(&sampler);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                sampler.sample()
            })
        })
        .collect::<Vec<_>>();
    barrier.wait();
    for thread in threads {
        assert_eq!(
            thread.join().unwrap(),
            Err(AiError::Capacity("hardware sample"))
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(sampler.metrics().sample_failures, 1);
}

#[test]
fn hardware_sampler_performs_no_idle_polling() {
    #[derive(Debug)]
    struct IdleProbe(Arc<AtomicUsize>);

    impl HardwareProbe for IdleProbe {
        fn sample(&self) -> AiResult<ResourceSnapshot> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(snapshot(Some(10)))
        }
    }

    let calls = Arc::new(AtomicUsize::new(0));
    let sampler =
        HardwareSampler::new(IdleProbe(Arc::clone(&calls)), Duration::from_millis(1)).unwrap();
    std::thread::sleep(Duration::from_millis(10));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(sampler.metrics().samples, 0);
}

fn dedicated_gpu(
    id: &str,
    total_gib: u64,
    available_gib: u64,
    load: u8,
    healthy: bool,
) -> DeviceSnapshot {
    let gib = 1024u64 * 1024 * 1024;
    DeviceSnapshot {
        id: DeviceId::new(id).unwrap(),
        kind: DeviceKind::Gpu,
        capabilities: DeviceCapabilities {
            class: DeviceClass::DiscreteGpu,
            memory_kind: DeviceMemoryKind::Dedicated,
            compatible_apis: Vec::new(),
        },
        total_memory_bytes: Some(total_gib * gib),
        available_memory_bytes: Some(available_gib * gib),
        used_memory_bytes: Some(total_gib.saturating_sub(available_gib) * gib),
        utilization_percent: Some(load),
        healthy,
    }
}

fn unified_gpu(id: &str, total_gib: u64, available_gib: u64, healthy: bool) -> DeviceSnapshot {
    let mut device = dedicated_gpu(id, total_gib, available_gib, 10, healthy);
    device.capabilities.class = DeviceClass::IntegratedGpu;
    device.capabilities.memory_kind = DeviceMemoryKind::Unified;
    device
}

#[test]
fn artifact_digest_round_trips_and_cache_rejects_bad_bytes() {
    let bytes = b"bounded-model";
    let descriptor = model_descriptor("model/cache", bytes);
    let digest = descriptor.artifact.digest.to_string();
    assert_eq!(
        ArtifactDigest::parse_hex(&digest).unwrap(),
        descriptor.artifact.digest
    );

    let root = temporary_directory("cache");
    let cache = LocalArtifactCache::new(&root, 1024).unwrap();
    let path = cache.store(&descriptor.artifact, bytes).unwrap();
    assert!(path.starts_with(std::fs::canonicalize(&root).unwrap()));
    assert_eq!(cache.load(&descriptor.artifact).unwrap(), bytes);
    assert_eq!(
        cache.store(&descriptor.artifact, b"wrong"),
        Err(AiError::Integrity("artifact digest"))
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn concurrent_artifact_writers_are_idempotent_and_never_publish_partial_bytes() {
    let bytes = b"concurrent-bounded-model".to_vec();
    let identity = model_descriptor("model/concurrent-cache", &bytes).artifact;
    let root = temporary_directory("concurrent-cache");
    let cache = Arc::new(LocalArtifactCache::new(&root, 1024).unwrap());
    let barrier = Arc::new(std::sync::Barrier::new(33));
    let mut writers = Vec::new();
    for _ in 0..32 {
        let cache = Arc::clone(&cache);
        let barrier = Arc::clone(&barrier);
        let identity = identity.clone();
        let bytes = bytes.clone();
        writers.push(std::thread::spawn(move || {
            barrier.wait();
            cache.store(&identity, &bytes)
        }));
    }
    barrier.wait();
    for writer in writers {
        assert!(writer.join().unwrap().is_ok());
    }
    assert!(cache.contains(&identity).unwrap());
    assert_eq!(cache.load(&identity).unwrap(), bytes);
    assert!(std::fs::read_dir(&root).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn artifact_cache_rejects_symlinks_at_final_open() {
    use std::os::unix::fs::symlink;

    let bytes = b"bounded-symlink-target";
    let identity = model_descriptor("model/symlink-cache", bytes).artifact;
    let root = temporary_directory("symlink-cache");
    let cache = LocalArtifactCache::new(&root, 1024).unwrap();
    let target = root.join("outside.artifact");
    std::fs::write(&target, bytes).unwrap();
    symlink(&target, cache.path(identity.digest)).unwrap();
    assert_eq!(
        cache.load(&identity),
        Err(AiError::Integrity("artifact file open"))
    );
    assert_eq!(
        cache.load_range(&identity, 0, 1, 1),
        Err(AiError::Integrity("artifact file open"))
    );
    assert_eq!(
        cache.contains(&identity),
        Err(AiError::Integrity("artifact file open"))
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_artifact_and_incompatible_route_fail_explicitly() {
    let descriptor = model_descriptor("model/missing", b"missing");
    let root = temporary_directory("missing");
    let cache = LocalArtifactCache::new(&root, 1024).unwrap();
    assert_eq!(
        cache.load(&descriptor.artifact),
        Err(AiError::NotFound("artifact"))
    );
    assert!(
        !descriptor.supports_route(&BackendId::new("another-backend").unwrap(), DeviceKind::Cpu)
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn model_registry_is_concurrent_and_rejects_duplicate_ids() {
    let registry = Arc::new(ModelRegistry::new());
    let descriptor = model_descriptor("model/concurrent", b"model");
    let mut threads = Vec::new();
    for _ in 0..4 {
        let registry = Arc::clone(&registry);
        let descriptor = descriptor.clone();
        threads.push(std::thread::spawn(move || {
            registry.register(descriptor, [ArtifactLocation::LocalStorage])
        }));
    }
    let results = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        registry.get(&descriptor.id).unwrap().state,
        ModelState::Available
    );
}

#[test]
fn model_lifecycle_and_locations_are_explicit() {
    let registry = ModelRegistry::new();
    let descriptor = model_descriptor("model/lifecycle", b"model");
    registry.register(descriptor.clone(), []).unwrap();
    registry
        .add_location(&descriptor.id, ArtifactLocation::Memory)
        .unwrap();
    registry
        .transition(&descriptor.id, ModelState::Loading)
        .unwrap();
    registry
        .transition(&descriptor.id, ModelState::Ready)
        .unwrap();
    assert_eq!(
        registry.transition(&descriptor.id, ModelState::Available),
        Err(AiError::Conflict("model state transition"))
    );
    assert!(registry
        .remove_location(&descriptor.id, &ArtifactLocation::Memory)
        .unwrap());
}

#[test]
fn lightweight_path_normalizes_and_scores_rules_explicitly() {
    let limits = AiLimits::default();
    let engine = LightweightEngine::new(
        vec![TextRule {
            label: "route.greeting".into(),
            pattern: "hello".into(),
            output: "greeting".into(),
            matching: RuleMatch::Contains,
        }],
        limits,
        8_000,
    )
    .unwrap();
    let normalization = AiRequest::text(AiTask::TransformText, "  one\t two ", limits).unwrap();
    let outcome = engine
        .resolve(&normalization, &CancellationToken::new())
        .unwrap();
    assert!(matches!(
        outcome,
        LightweightOutcome::Handled {
            response: AiResponse {
                output: AiOutput::Text(ref text),
                ..
            },
            certainty: LightweightCertainty::Certain,
            escalate: false,
            ..
        } if text == "one two"
    ));

    let classification = AiRequest::text(AiTask::ClassifyText, "hello there", limits).unwrap();
    let outcome = engine
        .resolve(&classification, &CancellationToken::new())
        .unwrap();
    assert!(matches!(
        outcome,
        LightweightOutcome::Handled {
            certainty: LightweightCertainty::HeuristicScore(4545),
            escalate: true,
            ..
        }
    ));
}

#[test]
fn backend_registry_is_explicit_and_rejects_duplicates() {
    let registry = BackendRegistry::new();
    let backend: Arc<dyn InferenceBackend> = Arc::new(FakeBackend::new("native", false));
    registry.register(Arc::clone(&backend)).unwrap();
    assert_eq!(
        registry.register(backend),
        Err(AiError::Conflict("backend id"))
    );
}

#[test]
fn model_backend_and_scheduler_maps_fail_at_fixed_capacity() {
    let backends = BackendRegistry::new();
    for index in 0..256 {
        backends
            .register(Arc::new(FakeBackend::new(
                &format!("backend/cap-{index}"),
                false,
            )))
            .unwrap();
    }
    assert_eq!(
        backends.register(Arc::new(FakeBackend::new("backend/cap-overflow", false))),
        Err(AiError::Capacity("backend registry"))
    );

    let models = ModelRegistry::new();
    for index in 0..4_096 {
        models
            .register(
                model_descriptor(&format!("model/cap-{index}"), b"shared"),
                [ArtifactLocation::Memory],
            )
            .unwrap();
    }
    assert_eq!(
        models.register(
            model_descriptor("model/cap-overflow", b"shared"),
            [ArtifactLocation::Memory],
        ),
        Err(AiError::Capacity("model registry"))
    );

    let scheduler = CostScheduler::default();
    for index in 0..4_096 {
        let key = placement_candidate(
            &format!("backend/learned-{index}"),
            ComputeTarget::LocalCpu(DeviceId::new("cpu/learned").unwrap()),
            true,
        )
        .key;
        scheduler.observe(key, PlacementMetrics::default()).unwrap();
    }
    let overflow = placement_candidate(
        "backend/learned-overflow",
        ComputeTarget::LocalCpu(DeviceId::new("cpu/learned").unwrap()),
        true,
    )
    .key;
    assert_eq!(
        scheduler.observe(overflow, PlacementMetrics::default()),
        Err(AiError::Capacity("scheduler learned routes"))
    );
}

#[test]
fn residency_records_and_pending_loads_fail_at_fixed_capacity() {
    let residents = ResidencyPlanner::new(
        ResidencyConfig::default(),
        vec![TierCapacity {
            tier: ResidencyTier::Memory,
            capacity_bytes: 8_192,
        }],
    )
    .unwrap();
    for index in 0..4_096 {
        residents
            .register(ResidencyRecord {
                model: ModelId::new(format!("model/resident-{index}")).unwrap(),
                tier: ResidencyTier::Memory,
                size_bytes: 1,
                last_used_ms: 0,
                use_count: 0,
                load_time_ms: 1,
                importance_basis_points: 1,
                predicted_next_use_ms: None,
            })
            .unwrap();
    }
    assert_eq!(
        residents.register(ResidencyRecord {
            model: ModelId::new("model/resident-overflow").unwrap(),
            tier: ResidencyTier::Memory,
            size_bytes: 1,
            last_used_ms: 0,
            use_count: 0,
            load_time_ms: 1,
            importance_basis_points: 1,
            predicted_next_use_ms: None,
        }),
        Err(AiError::Capacity("resident model records"))
    );

    let pending = ResidencyPlanner::new(
        ResidencyConfig::default(),
        vec![TierCapacity {
            tier: ResidencyTier::Memory,
            capacity_bytes: 1_024,
        }],
    )
    .unwrap();
    let mut reservations = Vec::new();
    for index in 0..256 {
        let decision = pending
            .begin(ResidencyRequest {
                model: ModelId::new(format!("model/pending-{index}")).unwrap(),
                preferred: ResidencyTier::Memory,
                fallbacks: Vec::new(),
                size_bytes: 1,
                load_time_ms: 1,
                importance_basis_points: 1,
                predicted_next_use_ms: None,
                now_ms: 0,
                resource_mode: AiResourceMode::Unrestricted,
                capacity_limit_bytes: None,
                prefetch: false,
                cancellation: CancellationToken::new(),
            })
            .unwrap();
        let ResidencyDecision::Reserved(reservation) = decision else {
            panic!("unique pending model was not reserved");
        };
        reservations.push(reservation);
    }
    assert_eq!(
        pending.begin(ResidencyRequest {
            model: ModelId::new("model/pending-overflow").unwrap(),
            preferred: ResidencyTier::Memory,
            fallbacks: Vec::new(),
            size_bytes: 1,
            load_time_ms: 1,
            importance_basis_points: 1,
            predicted_next_use_ms: None,
            now_ms: 0,
            resource_mode: AiResourceMode::Unrestricted,
            capacity_limit_bytes: None,
            prefetch: false,
            cancellation: CancellationToken::new(),
        }),
        Err(AiError::Capacity("residency reservations"))
    );
    assert_eq!(reservations.len(), 256);
}

#[test]
fn backend_registry_rejects_an_unsupported_input_modality() {
    let mut model = model_descriptor("model/vision", b"vision");
    model.tasks.push(AiTask::AnalyzeImage);
    model.input_modalities = vec![AiModality::Text, AiModality::Image];
    let request = AiRequest {
        task: AiTask::AnalyzeImage,
        input: AiInput::new(
            vec![AiContent::Binary {
                media_type: "image/jpeg".into(),
                bytes: vec![1],
            }],
            AiLimits::default(),
        )
        .unwrap(),
        options: AiOptions::default(),
    };

    let registry = BackendRegistry::new();
    let mut text_only = FakeBackend::new("native", false);
    text_only.descriptor.tasks.push(AiTask::AnalyzeImage);
    registry.register(Arc::new(text_only)).unwrap();
    assert!(registry.candidates(&request, &model).unwrap().is_empty());

    let registry = BackendRegistry::new();
    let mut vision = FakeBackend::new("native", false);
    vision.descriptor.tasks.push(AiTask::AnalyzeImage);
    vision.descriptor.input_modalities.push(AiModality::Image);
    registry.register(Arc::new(vision)).unwrap();
    assert_eq!(registry.candidates(&request, &model).unwrap().len(), 1);
}

#[test]
fn resolve_uses_backend_and_returns_safe_diagnostics() {
    let limits = AiLimits::default();
    let models = Arc::new(ModelRegistry::new());
    models
        .register(
            model_descriptor("model/resolve", b"model"),
            [ArtifactLocation::LocalStorage],
        )
        .unwrap();
    let backends = Arc::new(BackendRegistry::new());
    backends
        .register(Arc::new(FakeBackend::new("native", false)))
        .unwrap();
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000).unwrap()),
        Arc::clone(&models),
        backends,
        Arc::new(AlwaysAdmit),
    )
    .unwrap();
    let mut request = AiRequest::text(AiTask::GenerateText, "answer", limits).unwrap();
    request.options.include_diagnostics = true;
    let response = block_on(runtime.resolve(request)).unwrap();

    assert_eq!(response.output, AiOutput::Text("backend-answer".into()));
    assert!(matches!(
        response.decision,
        Some(ExecutionDecision {
            selected: ExecutionTarget::Local { .. },
            attempts,
            ..
        }) if attempts.len() == 1
    ));
    assert_eq!(
        models
            .get(&ModelId::new("model/resolve").unwrap())
            .unwrap()
            .state,
        ModelState::Ready
    );
}

#[test]
fn one_hundred_cold_requests_share_one_model_loader_and_all_complete() {
    let limits = AiLimits::default();
    let models = Arc::new(ModelRegistry::new());
    models
        .register(
            model_descriptor("model/cold-storm", b"model"),
            [ArtifactLocation::LocalStorage],
        )
        .unwrap();
    let backends = Arc::new(BackendRegistry::new());
    let backend =
        Arc::new(FakeBackend::new("native", false).with_load_delay(Duration::from_millis(10)));
    backends.register(backend.clone()).unwrap();
    let runtime = Arc::new(
        AiRuntime::new(
            limits,
            Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000).unwrap()),
            models,
            backends,
            Arc::new(AlwaysAdmit),
        )
        .unwrap()
        .with_execution_queue(ExecutionQueueConfig {
            max_active: 16,
            waiting: FairQueueConfig {
                capacity: 100,
                ..FairQueueConfig::default()
            },
            cancellation_poll: Duration::from_millis(1),
        })
        .unwrap(),
    );
    let barrier = Arc::new(std::sync::Barrier::new(101));
    let mut callers = Vec::new();
    for _ in 0..100 {
        let runtime = Arc::clone(&runtime);
        let barrier = Arc::clone(&barrier);
        callers.push(std::thread::spawn(move || {
            let mut request =
                AiRequest::text(AiTask::GenerateText, "answer", AiLimits::default()).unwrap();
            request.options.model = Some(ModelId::new("model/cold-storm").unwrap());
            barrier.wait();
            block_on(runtime.resolve(request))
        }));
    }
    barrier.wait();
    for caller in callers {
        assert!(caller.join().unwrap().is_ok());
    }
    assert_eq!(backend.load_count.load(Ordering::Relaxed), 1);
    assert_eq!(backend.inference_count.load(Ordering::Relaxed), 100);
    let loads = runtime.model_loads();
    assert_eq!(loads.loaders, 1);
    assert_eq!(loads.ready, 1);
    assert_eq!(loads.ready_hits, 99);
    assert!(loads.waiters > 0);
}

#[test]
fn backend_unavailability_invalidates_ready_load_for_next_request() {
    let limits = AiLimits::default();
    let models = Arc::new(ModelRegistry::new());
    let descriptor = model_descriptor("model/reload", b"model");
    models
        .register(descriptor.clone(), [ArtifactLocation::LocalStorage])
        .unwrap();
    let backends = Arc::new(BackendRegistry::new());
    let backend = Arc::new(FakeBackend::new("native", false));
    backends.register(backend.clone()).unwrap();
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000).unwrap()),
        models,
        backends,
        Arc::new(AlwaysAdmit),
    )
    .unwrap();
    let request = AiRequest::text(AiTask::GenerateText, "answer", limits).unwrap();
    block_on(runtime.resolve(request.clone())).unwrap();
    block_on(backend.unload(&descriptor, &CancellationToken::new())).unwrap();
    assert!(matches!(
        block_on(runtime.resolve(request.clone())),
        Err(AiError::NotFound("compatible AI route"))
    ));
    block_on(runtime.resolve(request)).unwrap();
    assert_eq!(backend.load_count.load(Ordering::Relaxed), 2);
    assert_eq!(runtime.model_loads().invalidations, 1);
}

#[test]
fn resolve_escalates_once_and_swarm_fails_closed_without_bridge() {
    let limits = AiLimits::default();
    let models = Arc::new(ModelRegistry::new());
    let mut descriptor = model_descriptor("model/escalate", b"model");
    descriptor.supported_backends = vec![
        BackendId::new("backend/a").unwrap(),
        BackendId::new("backend/b").unwrap(),
    ];
    models
        .register(descriptor, [ArtifactLocation::LocalStorage])
        .unwrap();
    let backends = Arc::new(BackendRegistry::new());
    let first_backend = Arc::new(FakeBackend::new("backend/a", true));
    let second_backend = Arc::new(FakeBackend::new("backend/b", false));
    backends.register(first_backend.clone()).unwrap();
    backends.register(second_backend.clone()).unwrap();
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000).unwrap()),
        models,
        backends,
        Arc::new(AlwaysAdmit),
    )
    .unwrap();
    let mut request = AiRequest::text(AiTask::GenerateText, "answer", limits).unwrap();
    request.options.include_diagnostics = true;
    let response = block_on(runtime.resolve(request)).unwrap();
    assert!(matches!(
        response.decision,
        Some(ExecutionDecision { attempts, .. }) if attempts.len() == 2
    ));
    assert_eq!(first_backend.load_count.load(Ordering::Relaxed), 1);
    assert_eq!(second_backend.load_count.load(Ordering::Relaxed), 1);

    let repeated = AiRequest::text(AiTask::GenerateText, "answer again", limits).unwrap();
    block_on(runtime.resolve(repeated)).unwrap();
    assert_eq!(first_backend.load_count.load(Ordering::Relaxed), 1);
    assert_eq!(second_backend.load_count.load(Ordering::Relaxed), 1);

    let mut swarm = AiRequest::text(AiTask::GenerateText, "answer", limits).unwrap();
    swarm.options.execution = AiExecutionMode::Swarm;
    swarm.options.distribution.allow_remote_compute = true;
    swarm.options.authorization = Some(remote_authorization(false));
    assert_eq!(
        block_on(runtime.resolve(swarm)),
        Err(AiError::SwarmUnavailable)
    );
}

#[test]
fn local_route_requires_explicit_permission_for_peer_only_artifact() {
    let limits = AiLimits::default();
    let models = Arc::new(ModelRegistry::new());
    models
        .register(
            model_descriptor("model/peer-only", b"peer-only"),
            [ArtifactLocation::Peer(PeerId::new("peer/storage").unwrap())],
        )
        .unwrap();
    let backends = Arc::new(BackendRegistry::new());
    backends
        .register(Arc::new(FakeBackend::new("native", false)))
        .unwrap();
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000).unwrap()),
        models,
        backends,
        Arc::new(AlwaysAdmit),
    )
    .unwrap();
    let mut request = AiRequest::text(AiTask::GenerateText, "answer", limits).unwrap();
    request.options.execution = AiExecutionMode::Local;

    assert_eq!(
        block_on(runtime.resolve(request)),
        Err(AiError::NotFound("compatible AI route"))
    );
}

fn remote_authorization(storage: bool) -> AiAuthorizationContext {
    let mut grants = vec![CapabilityId::new(REMOTE_COMPUTE_GRANT).unwrap()];
    if storage {
        grants.push(CapabilityId::new(REMOTE_STORAGE_GRANT).unwrap());
    }
    AiAuthorizationContext {
        tenant: CapabilityId::new("tenant/test").unwrap(),
        subject: CapabilityId::new("subject/test").unwrap(),
        grants,
    }
}

fn placement_candidate(backend: &str, target: ComputeTarget, resident: bool) -> PlacementCandidate {
    PlacementCandidate {
        key: PlacementKey {
            model: ModelId::new("model/scheduler").unwrap(),
            backend: BackendId::new(backend).unwrap(),
            target,
        },
        health: BackendHealth::Healthy,
        resources: ResourceEstimate {
            memory_bytes: 1_024,
            vram_bytes: 1_024,
            workers: 1,
            ..ResourceEstimate::default()
        },
        metrics: PlacementMetrics {
            load_percent: Some(10),
            queue_depth: 0,
            available_memory_bytes: Some(8_192),
            available_vram_bytes: Some(8_192),
            latency_ema_ms: Some(10),
            throughput_ema: Some(10),
        },
        model_resident: resident,
        artifact_source: Some(ArtifactLocation::LocalStorage),
        load_time_ms: 100,
        transfer_cost_units: 10,
        inference_cost_units: 10,
        rtt_ms: None,
        bandwidth_bytes_per_second: None,
        trusted: true,
        failover_cost_units: 5,
    }
}

fn placement_context(mode: AiResourceMode) -> PlacementContext {
    PlacementContext {
        priority: AiPriority::Normal,
        latency_class: AiLatencyClass::Balanced,
        resource_mode: mode,
        deadline_remaining: Some(Duration::from_secs(10)),
        allow_remote: false,
        prefer_local: true,
        max_remote_latency: Duration::from_millis(250),
        pressure_limited: false,
    }
}

#[test]
fn scheduler_prefers_hot_model_over_idle_cold_gpu_when_total_cost_is_lower() {
    let scheduler = CostScheduler::default();
    let mut hot = placement_candidate(
        "backend/hot",
        ComputeTarget::LocalGpu(DeviceId::new("gpu/hot").unwrap()),
        true,
    );
    hot.metrics.load_percent = Some(70);
    hot.metrics.queue_depth = 2;
    let mut cold = placement_candidate(
        "backend/cold",
        ComputeTarget::LocalGpu(DeviceId::new("gpu/cold").unwrap()),
        false,
    );
    cold.metrics.load_percent = Some(0);
    cold.load_time_ms = 1_000;
    cold.transfer_cost_units = 500;

    let plan = scheduler.plan(
        placement_context(AiResourceMode::Balanced),
        &[cold, hot.clone()],
    );
    assert_eq!(plan.ordered[0].key, hot.key);
}

#[test]
fn scheduler_rejects_vram_shortage_and_pressure_except_unrestricted() {
    let scheduler = CostScheduler::default();
    let mut gpu = placement_candidate(
        "backend/gpu",
        ComputeTarget::LocalGpu(DeviceId::new("gpu/0").unwrap()),
        true,
    );
    gpu.metrics.available_vram_bytes = Some(512);
    let plan = scheduler.plan(
        placement_context(AiResourceMode::Performance),
        &[gpu.clone()],
    );
    assert_eq!(plan.rejected[0].reason, PlacementRejectionReason::Vram);

    gpu.metrics.available_vram_bytes = Some(8_192);
    gpu.metrics.load_percent = Some(95);
    let mut balanced = placement_context(AiResourceMode::Balanced);
    balanced.pressure_limited = true;
    assert_eq!(
        scheduler.plan(balanced, &[gpu.clone()]).rejected[0].reason,
        PlacementRejectionReason::Pressure
    );
    let mut unrestricted = placement_context(AiResourceMode::Unrestricted);
    unrestricted.pressure_limited = true;
    assert_eq!(scheduler.plan(unrestricted, &[gpu]).ordered.len(), 1);
}

#[test]
fn scheduler_can_choose_cheaper_cpu_and_priority_reduces_queue_penalty() {
    let scheduler = CostScheduler::default();
    let mut cpu = placement_candidate(
        "backend/cpu",
        ComputeTarget::LocalCpu(DeviceId::new("cpu/0").unwrap()),
        false,
    );
    cpu.resources.vram_bytes = 0;
    cpu.inference_cost_units = 1;
    let mut gpu = placement_candidate(
        "backend/gpu",
        ComputeTarget::LocalGpu(DeviceId::new("gpu/0").unwrap()),
        false,
    );
    gpu.inference_cost_units = 100;
    let plan = scheduler.plan(
        placement_context(AiResourceMode::Balanced),
        &[gpu, cpu.clone()],
    );
    assert_eq!(plan.ordered[0].key, cpu.key);

    cpu.metrics.queue_depth = 16;
    let normal = scheduler.plan(placement_context(AiResourceMode::Balanced), &[cpu.clone()]);
    let mut high_context = placement_context(AiResourceMode::Balanced);
    high_context.priority = AiPriority::High;
    let high = scheduler.plan(high_context, &[cpu]);
    assert!(high.ordered[0].score < normal.ordered[0].score);
}

#[test]
fn scheduler_adapts_between_interactive_latency_and_aggregate_throughput() {
    let scheduler = CostScheduler::default();
    let mut fast = placement_candidate(
        "backend/fast",
        ComputeTarget::LocalGpu(DeviceId::new("gpu/fast").unwrap()),
        false,
    );
    fast.metrics.latency_ema_ms = Some(5);
    fast.metrics.throughput_ema = Some(1);
    let mut batched = placement_candidate(
        "backend/batched",
        ComputeTarget::LocalGpu(DeviceId::new("gpu/batched").unwrap()),
        false,
    );
    batched.metrics.latency_ema_ms = Some(50);
    batched.metrics.throughput_ema = Some(100);

    let mut interactive = placement_context(AiResourceMode::Balanced);
    interactive.latency_class = AiLatencyClass::Interactive;
    assert_eq!(
        scheduler
            .plan(interactive, &[batched.clone(), fast.clone()])
            .ordered[0]
            .key,
        fast.key
    );

    let mut throughput = placement_context(AiResourceMode::Balanced);
    throughput.latency_class = AiLatencyClass::Throughput;
    assert_eq!(
        scheduler.plan(throughput, &[fast, batched.clone()]).ordered[0].key,
        batched.key
    );
}

#[test]
fn scheduler_honors_local_preference_for_otherwise_equal_routes() {
    let scheduler = CostScheduler::default();
    let local = placement_candidate(
        "backend/local",
        ComputeTarget::LocalGpu(DeviceId::new("gpu/local").unwrap()),
        true,
    );
    let mut remote = placement_candidate(
        "backend/remote",
        ComputeTarget::RemotePeer {
            peer: PeerId::new("peer/remote").unwrap(),
            device: DeviceId::new("gpu/remote").unwrap(),
            kind: DeviceKind::Gpu,
        },
        true,
    );
    remote.rtt_ms = Some(0);
    let mut context = placement_context(AiResourceMode::Balanced);
    context.allow_remote = true;
    context.prefer_local = true;

    assert_eq!(
        scheduler.plan(context, &[remote, local.clone()]).ordered[0].key,
        local.key
    );
}

#[test]
fn scheduler_ema_is_fixed_point_and_deterministic() {
    let scheduler = CostScheduler::new(SchedulerWeights::default(), 5_000).unwrap();
    let candidate = placement_candidate(
        "backend/ema",
        ComputeTarget::LocalCpu(DeviceId::new("cpu/ema").unwrap()),
        false,
    );
    scheduler
        .observe(
            candidate.key.clone(),
            PlacementMetrics {
                latency_ema_ms: Some(100),
                throughput_ema: Some(20),
                ..PlacementMetrics::default()
            },
        )
        .unwrap();
    scheduler
        .observe(
            candidate.key.clone(),
            PlacementMetrics {
                latency_ema_ms: Some(200),
                throughput_ema: Some(40),
                ..PlacementMetrics::default()
            },
        )
        .unwrap();
    let first = scheduler.plan(
        placement_context(AiResourceMode::Balanced),
        std::slice::from_ref(&candidate),
    );
    let second = scheduler.plan(placement_context(AiResourceMode::Balanced), &[candidate]);
    assert_eq!(first, second);
}

#[test]
fn fair_queue_is_bounded_cancellable_and_prevents_starvation() {
    let mut queue = FairQueue::new(FairQueueConfig {
        capacity: 2,
        starvation_after: Duration::from_millis(10),
        overload_retry_after: Duration::from_millis(5),
    })
    .unwrap();
    let background = CancellationToken::new();
    assert!(matches!(
        queue.enqueue(
            "background",
            AiPriority::Background,
            0,
            Some(Duration::from_secs(1)),
            background,
        ),
        QueueAdmission::Queued { sequence: 1 }
    ));
    let cancelled = CancellationToken::new();
    assert!(matches!(
        queue.enqueue(
            "critical",
            AiPriority::Critical,
            9,
            Some(Duration::from_secs(1)),
            cancelled.clone(),
        ),
        QueueAdmission::Queued { sequence: 2 }
    ));
    assert!(matches!(
        queue.enqueue(
            "overflow",
            AiPriority::High,
            9,
            None,
            CancellationToken::new(),
        ),
        QueueAdmission::Rejected {
            reason: QueueRejectionReason::Full,
            retry_after: Some(_),
            ..
        }
    ));
    cancelled.cancel();
    let selected = queue.dequeue(10).unwrap();
    assert_eq!(selected.item, "background");
    assert_eq!(queue.metrics().cancelled, 1);
    assert_eq!(queue.metrics().promoted, 1);
}

fn batch_key(model: &str) -> BatchKey {
    BatchKey {
        model: ModelId::new(model).unwrap(),
        backend: BackendId::new("backend/batch").unwrap(),
        device: DeviceId::new("cpu/batch").unwrap(),
        task: BatchTaskClass::GenerateText,
    }
}

fn batch_pressure(mode: AiResourceMode, pressure_limited: bool) -> BatchPressure {
    BatchPressure {
        resource_mode: mode,
        pressure_limited,
        available_memory_bytes: Some(1_024),
        available_vram_bytes: Some(1_024),
        estimated_item_memory_bytes: 1,
        estimated_item_vram_bytes: 0,
        device_load_percent: Some(10),
    }
}

#[test]
fn dynamic_batcher_separates_keys_and_adapts_to_pressure() {
    let mut batcher = DynamicBatcher::new(DynamicBatcherConfig {
        max_queues: 2,
        max_total_items: 4,
        max_queue_depth: 3,
        max_batch_size: 2,
        max_wait: Duration::from_millis(10),
        overload_retry_after: Duration::from_millis(5),
    })
    .unwrap();
    let first_key = batch_key("model/batch-a");
    let second_key = batch_key("model/batch-b");
    for item in ["one", "two"] {
        assert!(matches!(
            batcher.enqueue(
                first_key.clone(),
                item,
                0,
                Some(Duration::from_secs(1)),
                CancellationToken::new(),
            ),
            BatchAdmission::Queued { .. }
        ));
    }
    batcher.enqueue(
        second_key.clone(),
        "other-model",
        0,
        None,
        CancellationToken::new(),
    );
    let first = batcher
        .take_ready(
            &first_key,
            1,
            batch_pressure(AiResourceMode::Performance, false),
            false,
        )
        .unwrap();
    assert_eq!(first.items.len(), 2);
    assert!(batcher
        .take_ready(
            &second_key,
            1,
            batch_pressure(AiResourceMode::Performance, false),
            false,
        )
        .is_none());
    let pressure_batch = batcher
        .take_ready(
            &second_key,
            1,
            batch_pressure(AiResourceMode::Balanced, true),
            false,
        )
        .unwrap();
    assert_eq!(pressure_batch.items.len(), 1);
}

#[test]
fn dynamic_batcher_honors_latency_and_backend_batch_ceilings() {
    let mut batcher = DynamicBatcher::new(DynamicBatcherConfig {
        max_queues: 1,
        max_total_items: 8,
        max_queue_depth: 8,
        max_batch_size: 8,
        max_wait: Duration::from_millis(10),
        overload_retry_after: Duration::from_millis(5),
    })
    .unwrap();
    let key = batch_key("model/policy");
    for item in 0..8 {
        batcher.enqueue(key.clone(), item, 0, None, CancellationToken::new());
    }
    let interactive = batcher
        .take_ready_with_policy(
            &key,
            0,
            BatchDispatchPolicy {
                pressure: batch_pressure(AiResourceMode::Performance, false),
                latency_class: AiLatencyClass::Interactive,
                backend_max_batch_size: Some(4),
            },
            false,
        )
        .unwrap();
    assert_eq!(interactive.items.len(), 1);
    let throughput = batcher
        .take_ready_with_policy(
            &key,
            0,
            BatchDispatchPolicy {
                pressure: batch_pressure(AiResourceMode::Performance, false),
                latency_class: AiLatencyClass::Throughput,
                backend_max_batch_size: Some(3),
            },
            false,
        )
        .unwrap();
    assert_eq!(throughput.items.len(), 3);
}

#[test]
fn dynamic_batcher_reacts_to_exact_vram_and_device_load() {
    let mut batcher = DynamicBatcher::new(DynamicBatcherConfig {
        max_queues: 1,
        max_total_items: 16,
        max_queue_depth: 16,
        max_batch_size: 8,
        max_wait: Duration::from_millis(10),
        overload_retry_after: Duration::from_millis(5),
    })
    .unwrap();
    let key = batch_key("model/gpu-pressure");
    for item in 0..12 {
        batcher.enqueue(key.clone(), item, 0, None, CancellationToken::new());
    }
    let mut vram_pressure = batch_pressure(AiResourceMode::Performance, false);
    vram_pressure.available_vram_bytes = Some(2);
    vram_pressure.estimated_item_vram_bytes = 1;
    let first = batcher.take_ready(&key, 0, vram_pressure, false).unwrap();
    assert_eq!(first.items.len(), 2);

    let mut load_pressure = batch_pressure(AiResourceMode::Performance, false);
    load_pressure.device_load_percent = Some(95);
    let second = batcher.take_ready(&key, 0, load_pressure, false).unwrap();
    assert_eq!(second.items.len(), 4);
}

#[test]
fn dynamic_batcher_enforces_backpressure_deadline_and_partial_metrics() {
    let mut batcher = DynamicBatcher::new(DynamicBatcherConfig {
        max_queues: 1,
        max_total_items: 1,
        max_queue_depth: 1,
        max_batch_size: 1,
        max_wait: Duration::from_millis(10),
        overload_retry_after: Duration::from_millis(7),
    })
    .unwrap();
    let key = batch_key("model/bounded");
    batcher.enqueue(key.clone(), "accepted", 0, None, CancellationToken::new());
    assert!(matches!(
        batcher.enqueue(
            key.clone(),
            "full",
            0,
            None,
            CancellationToken::new(),
        ),
        BatchAdmission::Rejected {
            reason: BatchRejectionReason::QueueFull,
            retry_after: Some(value),
            ..
        } if value == Duration::from_millis(7)
    ));
    let batch = batcher
        .take_ready(&key, 0, batch_pressure(AiResourceMode::Eco, false), false)
        .unwrap();
    assert_eq!(batch.items.len(), 1);
    batcher.record_outcomes(&[
        BatchItemOutcome {
            sequence: 1,
            result: Ok("ok"),
        },
        BatchItemOutcome::<&str> {
            sequence: 2,
            result: Err(AiError::BackendFailure {
                backend: BackendId::new("backend/batch").unwrap(),
                code: "sample",
            }),
        },
    ]);
    assert_eq!(batcher.metrics().partial_failures, 1);

    let cancelled = CancellationToken::new();
    cancelled.cancel();
    assert!(matches!(
        batcher.enqueue(key, "cancelled", 10, None, cancelled),
        BatchAdmission::Rejected {
            reason: BatchRejectionReason::Cancelled,
            retry_after: None,
            ..
        }
    ));
}

#[derive(Debug, Default)]
struct FakePeerArtifacts {
    bytes: Mutex<BTreeMap<ArtifactDigest, Vec<u8>>>,
}

impl PeerArtifactTransport for FakePeerArtifacts {
    fn contains(&self, _peer: &PeerId, identity: &ArtifactIdentity) -> AiResult<bool> {
        Ok(self
            .bytes
            .lock()
            .map_err(|_| AiError::InternalState)?
            .contains_key(&identity.digest))
    }

    fn fetch(
        &self,
        _peer: &PeerId,
        identity: &ArtifactIdentity,
        _max_bytes: u64,
        cancellation: &CancellationToken,
    ) -> AiResult<Vec<u8>> {
        if cancellation.is_cancelled() {
            return Err(AiError::Cancelled);
        }
        self.bytes
            .lock()
            .map_err(|_| AiError::InternalState)?
            .get(&identity.digest)
            .cloned()
            .ok_or(AiError::NotFound("peer artifact"))
    }

    fn put(
        &self,
        _peer: &PeerId,
        identity: &ArtifactIdentity,
        bytes: &[u8],
        _cancellation: &CancellationToken,
    ) -> AiResult<()> {
        self.bytes
            .lock()
            .map_err(|_| AiError::InternalState)?
            .insert(identity.digest, bytes.to_vec());
        Ok(())
    }

    fn remove(&self, _peer: &PeerId, identity: &ArtifactIdentity) -> AiResult<bool> {
        Ok(self
            .bytes
            .lock()
            .map_err(|_| AiError::InternalState)?
            .remove(&identity.digest)
            .is_some())
    }
}

#[test]
fn tiered_artifact_store_promotes_verified_peer_bytes() {
    let bytes = b"peer-model";
    let identity = model_descriptor("model/peer", bytes).artifact;
    let transport = Arc::new(FakePeerArtifacts::default());
    transport
        .put(
            &PeerId::new("peer/a").unwrap(),
            &identity,
            bytes,
            &CancellationToken::new(),
        )
        .unwrap();
    let peer = Arc::new(
        PeerArtifactStore::new(
            ArtifactStoreDescriptor {
                kind: ArtifactStoreKind::Peer(PeerId::new("peer/a").unwrap()),
                healthy: true,
                trusted: true,
                latency: Some(Duration::from_millis(10)),
                bandwidth_bytes_per_second: Some(1_000_000),
                reliability_basis_points: 9_900,
            },
            transport,
        )
        .unwrap(),
    );
    let peer_tier: Arc<dyn ArtifactStore> = peer.clone();
    let memory = Arc::new(MemoryArtifactStore::new(1_024).unwrap());
    let fast: Arc<dyn ArtifactStore> = memory.clone();
    let tiered = TieredArtifactStore::new(vec![fast, peer_tier], 1_024).unwrap();
    let loaded = tiered
        .load_and_promote(&identity, 1_024, &CancellationToken::new())
        .unwrap();
    assert_eq!(loaded, bytes);
    assert!(memory.contains(&identity).unwrap());
    assert_eq!(
        peer.metrics(),
        PeerArtifactMetrics {
            fetches: 1,
            transferred_bytes: u64::try_from(bytes.len()).unwrap(),
            failures: 0,
        }
    );
}

#[test]
fn peer_artifact_store_fails_closed_without_trust() {
    let identity = model_descriptor("model/untrusted", b"model").artifact;
    let store = PeerArtifactStore::new(
        ArtifactStoreDescriptor {
            kind: ArtifactStoreKind::Peer(PeerId::new("peer/untrusted").unwrap()),
            healthy: true,
            trusted: false,
            latency: None,
            bandwidth_bytes_per_second: None,
            reliability_basis_points: 5_000,
        },
        Arc::new(FakePeerArtifacts::default()),
    )
    .unwrap();
    assert_eq!(store.contains(&identity), Err(AiError::Unauthorized));
    assert_eq!(
        store.load(&identity, 1_024, &CancellationToken::new()),
        Err(AiError::Unauthorized)
    );
    assert_eq!(store.metrics().failures, 1);
}

fn residency_record(model: &str, tier: ResidencyTier, bytes: u64) -> ResidencyRecord {
    ResidencyRecord {
        model: ModelId::new(model).unwrap(),
        tier,
        size_bytes: bytes,
        last_used_ms: 1,
        use_count: 1,
        load_time_ms: 10,
        importance_basis_points: 1_000,
        predicted_next_use_ms: None,
    }
}

fn residency_request(
    model: &str,
    preferred: ResidencyTier,
    fallbacks: Vec<ResidencyTier>,
    bytes: u64,
    mode: AiResourceMode,
) -> ResidencyRequest {
    ResidencyRequest {
        model: ModelId::new(model).unwrap(),
        preferred,
        fallbacks,
        size_bytes: bytes,
        load_time_ms: 20,
        importance_basis_points: 5_000,
        predicted_next_use_ms: Some(100),
        now_ms: 10,
        resource_mode: mode,
        capacity_limit_bytes: None,
        prefetch: false,
        cancellation: CancellationToken::new(),
    }
}

#[test]
fn residency_eviction_commits_only_after_success_and_hot_model_reuses() {
    let vram = ResidencyTier::Vram(DeviceId::new("gpu/residency").unwrap());
    let planner = ResidencyPlanner::new(
        ResidencyConfig {
            max_fill_basis_points: 10_000,
            ..ResidencyConfig::default()
        },
        vec![TierCapacity {
            tier: vram.clone(),
            capacity_bytes: 100,
        }],
    )
    .unwrap();
    planner
        .register(residency_record("model/old", vram.clone(), 80))
        .unwrap();
    let request = residency_request(
        "model/new",
        vram.clone(),
        Vec::new(),
        60,
        AiResourceMode::Balanced,
    );
    let first = match planner.begin(request.clone()).unwrap() {
        ResidencyDecision::Reserved(value) => value,
        other => panic!("unexpected residency decision: {other:?}"),
    };
    assert_eq!(first.evictions.len(), 1);
    assert!(matches!(
        planner.begin(request.clone()).unwrap(),
        ResidencyDecision::InFlight { .. }
    ));
    planner.finish(first, false, 20).unwrap();
    assert_eq!(planner.snapshot().unwrap()[0].model.as_str(), "model/old");

    let second = match planner.begin(request.clone()).unwrap() {
        ResidencyDecision::Reserved(value) => value,
        other => panic!("unexpected residency decision: {other:?}"),
    };
    planner.finish(second, true, 30).unwrap();
    assert_eq!(planner.snapshot().unwrap()[0].model.as_str(), "model/new");
    assert!(matches!(
        planner.begin(request).unwrap(),
        ResidencyDecision::Reuse { tier } if tier == vram
    ));
    assert_eq!(
        planner.metrics().unwrap(),
        ResidencyMetrics {
            reuses: 1,
            in_flight: 1,
            reservations: 2,
            rollbacks: 1,
            evictions: 1,
            residents: 1,
            resident_bytes: 60,
            pending: 0,
            active_prefetch: 0,
        }
    );
}

#[test]
fn residency_degrades_tier_and_unrestricted_uses_declared_capacity() {
    let vram = ResidencyTier::Vram(DeviceId::new("gpu/small").unwrap());
    let planner = ResidencyPlanner::new(
        ResidencyConfig {
            max_fill_basis_points: 5_000,
            ..ResidencyConfig::default()
        },
        vec![
            TierCapacity {
                tier: vram.clone(),
                capacity_bytes: 50,
            },
            TierCapacity {
                tier: ResidencyTier::Memory,
                capacity_bytes: 100,
            },
        ],
    )
    .unwrap();
    assert!(matches!(
        planner.begin(residency_request(
            "model/balanced-too-large",
            vram.clone(),
            vec![ResidencyTier::Memory],
            80,
            AiResourceMode::Balanced,
        )),
        Err(AiError::Capacity("model residency"))
    ));
    let reservation = match planner
        .begin(residency_request(
            "model/unrestricted",
            vram,
            vec![ResidencyTier::Memory],
            80,
            AiResourceMode::Unrestricted,
        ))
        .unwrap()
    {
        ResidencyDecision::Reserved(value) => value,
        other => panic!("unexpected residency decision: {other:?}"),
    };
    assert_eq!(reservation.target, ResidencyTier::Memory);
}

#[cfg(feature = "backend-candle")]
fn candle_model() -> (NativeLinearArtifact, ModelDescriptor) {
    let dimensions = 256;
    let mut weights = vec![0.0; dimensions * 2];
    weights[usize::from(b'a')] = 10.0;
    weights[dimensions + usize::from(b'b')] = 10.0;
    let artifact = NativeLinearArtifact::new(
        dimensions,
        vec!["class-a".into(), "class-b".into()],
        weights,
        vec![0.0, 0.0],
    )
    .unwrap();
    let bytes = artifact.encode().unwrap();
    let descriptor = ModelDescriptor {
        id: ModelId::new("model/candle-linear").unwrap(),
        revision: "v1".into(),
        tasks: vec![AiTask::ClassifyText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::NativeLinearV1,
        quantization: Quantization::None,
        estimated_memory_bytes: u64::try_from(bytes.len() * 2).unwrap(),
        estimated_vram_bytes: 0,
        max_input_bytes: 1_024,
        max_output_bytes: 1_024,
        context_limit: None,
        supported_backends: vec![BackendId::new(CANDLE_LINEAR_BACKEND_ID).unwrap()],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 10,
        quality: Some(QualityTier::Tiny),
        artifact: artifact.identity(None, false).unwrap(),
    };
    (artifact, descriptor)
}

#[cfg(feature = "backend-candle")]
#[test]
fn candle_backend_loads_inferrs_unloads_and_is_thread_safe() {
    let (artifact, descriptor) = candle_model();
    let bytes = artifact.encode().unwrap();
    let memory = Arc::new(MemoryArtifactStore::new(1024 * 1024).unwrap());
    memory
        .store(&descriptor.artifact, &bytes, &CancellationToken::new())
        .unwrap();
    let store: Arc<dyn ArtifactStore> = memory;
    let backend = Arc::new(CandleBackend::new(store, CandleBackendConfig::default()).unwrap());
    block_on(backend.load(&descriptor, &CancellationToken::new())).unwrap();
    let mut threads = Vec::new();
    for _ in 0..8 {
        let backend = Arc::clone(&backend);
        let descriptor = descriptor.clone();
        threads.push(std::thread::spawn(move || {
            let request = AiRequest::text(AiTask::ClassifyText, "a", AiLimits::default()).unwrap();
            let response = block_on(backend.infer(
                &request,
                &descriptor,
                &DeviceId::new("local/cpu/candle").unwrap(),
                &CancellationToken::new(),
            ))
            .unwrap();
            match response.output {
                AiOutput::Scores(scores) => scores[0].score > scores[1].score,
                _ => false,
            }
        }));
    }
    assert!(threads.into_iter().all(|thread| thread.join().unwrap()));
    let mut batch = (0..8)
        .map(|_| AiRequest::text(AiTask::ClassifyText, "a", AiLimits::default()).unwrap())
        .collect::<Vec<_>>();
    batch[3] = AiRequest::text(AiTask::GenerateText, "a", AiLimits::default()).unwrap();
    let outcomes = block_on(backend.infer_batch(
        &batch,
        &descriptor,
        &DeviceId::new("local/cpu/candle").unwrap(),
        &CancellationToken::new(),
    ))
    .unwrap();
    assert_eq!(outcomes.len(), 8);
    assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 7);
    assert!(matches!(
        outcomes[3],
        Err(AiError::Incompatible("Candle linear task"))
    ));
    let oversized = (0..=CANDLE_LINEAR_MAX_BATCH_SIZE)
        .map(|_| AiRequest::text(AiTask::ClassifyText, "a", AiLimits::default()).unwrap())
        .collect::<Vec<_>>();
    assert!(matches!(
        block_on(backend.infer_batch(
            &oversized,
            &descriptor,
            &DeviceId::new("local/cpu/candle").unwrap(),
            &CancellationToken::new(),
        )),
        Err(AiError::LimitExceeded {
            kind: LimitKind::InputParts,
            ..
        })
    ));
    block_on(backend.unload(&descriptor, &CancellationToken::new())).unwrap();
    let request = AiRequest::text(AiTask::ClassifyText, "a", AiLimits::default()).unwrap();
    assert!(matches!(
        block_on(backend.infer(
            &request,
            &descriptor,
            &DeviceId::new("local/cpu/candle").unwrap(),
            &CancellationToken::new(),
        )),
        Err(AiError::BackendUnavailable(_))
    ));
}

#[cfg(feature = "backend-candle")]
#[test]
fn candle_artifact_rejects_truncation_and_required_unverified_signature() {
    let (artifact, mut descriptor) = candle_model();
    let mut bytes = artifact.encode().unwrap();
    bytes.pop();
    assert!(NativeLinearArtifact::decode(&bytes, 1_024, 10).is_err());

    descriptor.artifact.signature_required = true;
    let memory = Arc::new(MemoryArtifactStore::new(1024 * 1024).unwrap());
    memory
        .store(
            &descriptor.artifact,
            &artifact.encode().unwrap(),
            &CancellationToken::new(),
        )
        .unwrap();
    let store: Arc<dyn ArtifactStore> = memory;
    let backend = CandleBackend::new(store, CandleBackendConfig::default()).unwrap();
    assert_eq!(
        block_on(backend.load(&descriptor, &CancellationToken::new())),
        Err(AiError::Integrity("artifact signature not verified"))
    );
}

#[cfg(feature = "training-candle")]
#[derive(Debug)]
struct AlwaysTrain;

#[cfg(feature = "training-candle")]
impl TrainingAdmission for AlwaysTrain {
    fn admit(&self, _job: &TrainingJob) -> AiResult<AdmissionDecision> {
        Ok(AdmissionDecision::Admit {
            budget: ResourceBudget {
                cpu_percent: 100,
                gpu_percent: 0,
                memory_bytes: Some(1024 * 1024),
                vram_bytes: Some(0),
                storage_bytes: 1024 * 1024,
                workers: 2,
                concurrent_jobs: 1,
                pressure_limited: false,
            },
        })
    }
}

#[cfg(feature = "training-candle")]
#[derive(Debug)]
struct DeferredTraining;

#[cfg(feature = "training-candle")]
impl TrainingAdmission for DeferredTraining {
    fn admit(&self, _job: &TrainingJob) -> AiResult<AdmissionDecision> {
        Ok(AdmissionDecision::Defer {
            reason: AdmissionReason::MemoryPressure,
            retry_after: Duration::from_secs(1),
        })
    }
}

#[cfg(feature = "training-candle")]
#[derive(Debug, Default)]
struct ProgressLog(Mutex<Vec<TrainingProgress>>);

#[cfg(feature = "training-candle")]
impl TrainingProgressObserver for ProgressLog {
    fn report(&self, progress: &TrainingProgress) {
        if let Ok(mut entries) = self.0.lock() {
            entries.push(progress.clone());
        }
    }
}

#[cfg(feature = "training-candle")]
fn training_job() -> TrainingJob {
    TrainingJob {
        id: CapabilityId::new("training/test").unwrap(),
        model: ModelId::new("model/trained-linear").unwrap(),
        revision: "v1".into(),
        labels: vec!["class-a".into(), "class-b".into()],
        input_dimensions: 256,
        epochs: 20,
        max_steps: 100,
        batch_size: 2,
        learning_rate: 1.0,
        seed: 42,
        resource_requirements: ResourceEstimate {
            cpu_percent: 50,
            memory_bytes: 1024 * 1024,
            workers: 1,
            ..ResourceEstimate::default()
        },
        resource_mode: AiResourceMode::Balanced,
        checkpoints: TrainingCheckpointPolicy {
            every_epochs: 5,
            max_checkpoints: 2,
        },
        resume: None,
        publisher: Some(CapabilityId::new("publisher/test").unwrap()),
        max_input_bytes: 1_024,
        max_output_bytes: 1_024,
    }
}

#[cfg(feature = "training-candle")]
#[test]
fn deferred_training_never_runs_a_blind_minimum_batch() {
    assert_eq!(
        DeferredTraining.batch_limit(&training_job(), 8),
        Err(AiError::Capacity("training resources deferred"))
    );
}

#[cfg(feature = "training-candle")]
fn training_dataset() -> Arc<dyn TrainingDataset> {
    Arc::new(
        InMemoryTrainingDataset::new(
            vec![
                TrainingExample {
                    text: "a".into(),
                    label: 0,
                },
                TrainingExample {
                    text: "b".into(),
                    label: 1,
                },
                TrainingExample {
                    text: "a".into(),
                    label: 0,
                },
                TrainingExample {
                    text: "b".into(),
                    label: 1,
                },
            ],
            16,
            16,
        )
        .unwrap(),
    )
}

#[cfg(feature = "training-candle")]
#[test]
fn candle_training_is_bounded_reproducible_and_registry_ready() {
    let memory = Arc::new(MemoryArtifactStore::new(4 * 1024 * 1024).unwrap());
    let store: Arc<dyn ArtifactStore> = memory.clone();
    let trainer = CandleTrainer::new(
        Arc::clone(&store),
        Arc::new(AlwaysTrain),
        CandleTrainerConfig::default(),
    )
    .unwrap();
    let progress = Arc::new(ProgressLog::default());
    let observer: Arc<dyn TrainingProgressObserver> = progress.clone();
    let job = training_job();
    let first = block_on(trainer.train(
        &job,
        training_dataset(),
        observer,
        &CancellationToken::new(),
    ))
    .unwrap();
    let second = block_on(trainer.train(
        &job,
        training_dataset(),
        Arc::new(IgnoreTrainingProgress),
        &CancellationToken::new(),
    ))
    .unwrap();
    assert_eq!(first.bytes, second.bytes);
    assert_eq!(first.identity.publisher, job.publisher);
    assert_eq!(first.completed_epochs, 20);
    assert!(first.final_loss.is_finite());
    assert!(progress
        .0
        .lock()
        .unwrap()
        .iter()
        .any(|entry| entry.checkpoint.is_some()));

    let registry = ModelRegistry::new();
    registry
        .register(first.descriptor.clone(), [ArtifactLocation::Memory])
        .unwrap();
    let backend = CandleBackend::new(store, CandleBackendConfig::default()).unwrap();
    block_on(backend.load(&first.descriptor, &CancellationToken::new())).unwrap();
    let request = AiRequest::text(AiTask::ClassifyText, "a", AiLimits::default()).unwrap();
    let response = block_on(backend.infer(
        &request,
        &first.descriptor,
        &DeviceId::new("local/cpu/candle").unwrap(),
        &CancellationToken::new(),
    ))
    .unwrap();
    assert!(matches!(
        response.output,
        AiOutput::Scores(scores) if scores[0].score > scores[1].score
    ));
}

#[cfg(feature = "training-candle")]
#[test]
fn candle_training_cancels_and_resumes_from_verified_artifact() {
    let memory = Arc::new(MemoryArtifactStore::new(4 * 1024 * 1024).unwrap());
    let store: Arc<dyn ArtifactStore> = memory;
    let trainer = CandleTrainer::new(
        Arc::clone(&store),
        Arc::new(AlwaysTrain),
        CandleTrainerConfig::default(),
    )
    .unwrap();
    let job = training_job();
    let cancellation = CancellationToken::new();
    cancellation.cancel();
    assert!(matches!(
        block_on(trainer.train(
            &job,
            training_dataset(),
            Arc::new(IgnoreTrainingProgress),
            &cancellation,
        )),
        Err(AiError::Cancelled)
    ));

    let output = block_on(trainer.train(
        &job,
        training_dataset(),
        Arc::new(IgnoreTrainingProgress),
        &CancellationToken::new(),
    ))
    .unwrap();
    let mut resumed = job;
    resumed.epochs = 1;
    resumed.resume = Some(output.identity);
    let resumed_output = block_on(trainer.train(
        &resumed,
        training_dataset(),
        Arc::new(IgnoreTrainingProgress),
        &CancellationToken::new(),
    ))
    .unwrap();
    assert_eq!(resumed_output.completed_epochs, 1);
}

#[derive(Debug)]
struct FakeProvenanceVerifier;

impl ArtifactProvenanceVerifier for FakeProvenanceVerifier {
    fn verify(
        &self,
        _identity: &ArtifactIdentity,
        provenance: &ArtifactProvenance,
    ) -> AiResult<()> {
        if provenance.signature == b"valid-signature" {
            Ok(())
        } else {
            Err(AiError::Integrity("test signature"))
        }
    }
}

#[test]
fn signed_artifact_requires_valid_unexpired_provenance() {
    let bytes = b"signed-model";
    let publisher = CapabilityId::new("publisher/signed").unwrap();
    let identity = ArtifactIdentity {
        digest: ArtifactDigest::from_bytes(bytes),
        size_bytes: u64::try_from(bytes.len()).unwrap(),
        publisher: Some(publisher.clone()),
        signature_required: true,
    };
    let clock = Arc::new(StaticAiClock::new(100));
    let inner: Arc<dyn ArtifactStore> = Arc::new(MemoryArtifactStore::new(1_024).unwrap());
    let store = ProvenanceArtifactStore::new(
        inner,
        Arc::new(FakeProvenanceVerifier),
        clock.clone(),
        4,
        128,
    )
    .unwrap();
    store
        .register(
            &identity,
            ArtifactProvenance {
                publisher,
                signature: b"valid-signature".to_vec(),
                signed_at_ms: 50,
                expires_at_ms: 200,
            },
        )
        .unwrap();
    store
        .store(&identity, bytes, &CancellationToken::new())
        .unwrap();
    assert_eq!(
        store
            .load(&identity, 1_024, &CancellationToken::new())
            .unwrap(),
        bytes
    );
    clock.set(201);
    assert_eq!(
        store.load(&identity, 1_024, &CancellationToken::new()),
        Err(AiError::Integrity("artifact provenance"))
    );
}

#[test]
fn security_policy_rejects_provider_formats_and_debug_redacts_content() {
    let mut model = model_descriptor("model/unsafe", b"unsafe");
    model.format = ArtifactFormat::Other(CapabilityId::new("provider/custom-op").unwrap());
    assert_eq!(
        ModelSecurityPolicy::default().validate_model(&model),
        Err(AiError::Unauthorized)
    );

    let request = AiRequest::text(
        AiTask::GenerateText,
        "prompt-secret-value",
        AiLimits::default(),
    )
    .unwrap();
    assert!(!format!("{request:?}").contains("prompt-secret-value"));
    let response = AiResponse::new(
        AiOutput::Text("output-secret-value".into()),
        vec![AiMetadata {
            key: "safe-key".into(),
            value: "metadata-secret-value".into(),
        }],
        None,
        AiLimits::default(),
    )
    .unwrap();
    let debug = format!("{response:?}");
    assert!(!debug.contains("output-secret-value"));
    assert!(!debug.contains("metadata-secret-value"));
    assert!(AiResponse::new(
        AiOutput::Embedding(vec![f32::NAN]),
        Vec::new(),
        None,
        AiLimits::default(),
    )
    .is_err());
}

#[derive(Debug, Default)]
struct ObservationLog(Mutex<Vec<AiObservation>>);

impl AiObservationSink for ObservationLog {
    fn record(&self, observation: &AiObservation) {
        if let Ok(mut observations) = self.0.lock() {
            observations.push(observation.clone());
        }
    }
}

#[test]
fn runtime_observability_is_bounded_and_payload_free() {
    let limits = AiLimits::default();
    let models = Arc::new(ModelRegistry::new());
    models
        .register(
            model_descriptor("model/observed", b"model"),
            [ArtifactLocation::LocalStorage],
        )
        .unwrap();
    let backends = Arc::new(BackendRegistry::new());
    backends
        .register(Arc::new(FakeBackend::new("native", false)))
        .unwrap();
    let observations = Arc::new(ObservationLog::default());
    let sink: Arc<dyn AiObservationSink> = observations.clone();
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000).unwrap()),
        models,
        backends,
        Arc::new(AlwaysAdmit),
    )
    .unwrap()
    .with_observation_sink(sink);
    let request = AiRequest::text(AiTask::GenerateText, "private-prompt", limits).unwrap();
    block_on(runtime.resolve(request)).unwrap();
    let snapshot = runtime.telemetry();
    assert_eq!(snapshot.requests, 1);
    assert_eq!(snapshot.successes, 1);
    assert_eq!(snapshot.local_placements, 1);
    assert_eq!(snapshot.model_load_successes, 1);
    assert!(snapshot.latency_p50 >= Duration::from_millis(1));
    let events = observations.0.lock().unwrap();
    assert!(events
        .iter()
        .any(|event| matches!(event, AiObservation::RouteSelected { .. })));
    assert!(events
        .iter()
        .any(|event| matches!(event, AiObservation::RequestCompleted { attempts: 1, .. })));
    assert!(!format!("{events:?}").contains("private-prompt"));
}

#[test]
fn deterministic_boundary_property_sweep_never_bypasses_limits() {
    let limits = AiLimits {
        max_input_bytes: 64,
        max_input_parts: 2,
        ..AiLimits::default()
    };
    let mut state = 0x4d59_5df4_d0f3_3173u64;
    for length in 0..512usize {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let text = (0..length)
            .map(|index| {
                let value = state.rotate_left(u32::try_from(index % 64).unwrap()) as u8;
                char::from(b'a' + value % 26)
            })
            .collect::<String>();
        let result = AiRequest::text(AiTask::ClassifyText, text, limits);
        if length == 0 || length > limits.max_input_bytes {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().validate(limits).is_ok());
        }
    }
}

#[cfg(feature = "backend-candle")]
#[test]
fn native_artifact_corruption_fixture_and_byte_sweep_fail_safely() {
    let fixture = include_bytes!("../tests/fixtures/corrupt-native-linear-v1.artifact");
    assert!(NativeLinearArtifact::decode(fixture, 4_096, 256).is_err());
    let mut state = 0xa5a5_1234_9876_5a5au64;
    for length in 0..1_024usize {
        let mut bytes = Vec::with_capacity(length);
        for _ in 0..length {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            bytes.push(state as u8);
        }
        let _ = NativeLinearArtifact::decode(&bytes, 512, 32);
    }
}

#[cfg(feature = "swarm")]
#[derive(Debug)]
struct AllowPeer {
    allow: bool,
}

#[cfg(feature = "swarm")]
impl PeerCapabilityAuthorizer for AllowPeer {
    fn authorize(&self, capabilities: &AiNodeCapabilities) -> AiResult<PeerAuthorization> {
        Ok(PeerAuthorization {
            authenticated: self.allow,
            tenants: [capabilities.tenant.clone()].into_iter().collect(),
            allow_compute: self.allow,
            allow_storage: self.allow,
        })
    }
}

#[cfg(feature = "swarm")]
fn donated_budget(cpu: u8, storage: u64) -> ResourceBudget {
    ResourceBudget {
        cpu_percent: cpu,
        gpu_percent: 0,
        memory_bytes: Some(1_024),
        vram_bytes: Some(0),
        storage_bytes: storage,
        workers: usize::from(cpu > 0),
        concurrent_jobs: usize::from(cpu > 0),
        pressure_limited: false,
    }
}

#[cfg(feature = "swarm")]
fn contribution_policy(compute: bool, storage: bool) -> AiContributionPolicy {
    AiContributionPolicy {
        contribute_compute: compute,
        contribute_storage: storage,
        max_cpu_percent: if compute { 25 } else { 0 },
        max_gpu_percent: 0,
        max_memory_bytes: 1_024,
        max_vram_bytes: 0,
        max_storage_bytes: if storage { 4_096 } else { 0 },
        max_workers: usize::from(compute),
        max_concurrent_jobs: usize::from(compute),
    }
}

#[cfg(feature = "swarm")]
fn peer_capabilities(
    peer: &str,
    compute: bool,
    storage: bool,
    digest: ArtifactDigest,
) -> AiNodeCapabilities {
    let compute_devices = if compute {
        vec![AdvertisedCompute {
            backend: BackendId::new("remote/linear").unwrap(),
            device: DeviceId::new(format!("{peer}/cpu")).unwrap(),
            kind: DeviceKind::Cpu,
            metrics: PlacementMetrics {
                load_percent: Some(10),
                queue_depth: 0,
                available_memory_bytes: Some(1_024),
                available_vram_bytes: Some(0),
                latency_ema_ms: Some(10),
                throughput_ema: Some(10),
            },
        }]
    } else {
        Vec::new()
    };
    let storage_budget = if storage { 4_096 } else { 0 };
    AiNodeCapabilities::from_contribution(
        PeerId::new(peer).unwrap(),
        CapabilityId::new("tenant/test").unwrap(),
        0,
        1_000,
        compute_devices,
        storage.then_some(AdvertisedStorage {
            available_bytes: storage_budget,
            max_transfers: 1,
        }),
        storage.then_some(digest).into_iter().collect(),
        donated_budget(if compute { 25 } else { 0 }, storage_budget),
        contribution_policy(compute, storage),
    )
    .unwrap()
}

#[cfg(feature = "swarm")]
#[test]
fn swarm_advertisement_clamps_physical_device_to_donated_budget() {
    let device = dedicated_gpu("local/gpu/donated", 24, 20, 10, true);
    let advertised = AdvertisedCompute::from_device(
        BackendId::new("backend/donated").unwrap(),
        &device,
        ResourceBudget {
            cpu_percent: 25,
            gpu_percent: 25,
            memory_bytes: Some(2 * 1024 * 1024 * 1024),
            vram_bytes: Some(5 * 1024 * 1024 * 1024),
            storage_bytes: 0,
            workers: 1,
            concurrent_jobs: 1,
            pressure_limited: false,
        },
    );
    assert_eq!(
        advertised.metrics.available_vram_bytes,
        Some(5 * 1024 * 1024 * 1024)
    );
    assert_eq!(advertised.metrics.available_memory_bytes, None);
}

#[cfg(feature = "swarm")]
#[test]
fn peer_directory_supports_three_independent_contribution_profiles() {
    let digest = ArtifactDigest::from_bytes(b"peer-model");
    let directory = PeerCapabilityDirectory::new(PeerDirectoryConfig {
        max_ttl: Duration::from_secs(1),
        ..PeerDirectoryConfig::default()
    })
    .unwrap();
    let authorizer = AllowPeer { allow: true };
    for capabilities in [
        peer_capabilities("peer/storage", false, true, digest),
        peer_capabilities("peer/compute", true, false, digest),
        peer_capabilities("peer/combined", true, true, digest),
    ] {
        directory.update(capabilities, &authorizer, 1).unwrap();
    }
    let live = directory
        .live(&CapabilityId::new("tenant/test").unwrap(), 2, 3)
        .unwrap();
    assert_eq!(live.len(), 3);
    assert!(live.iter().any(|peer| peer.compute.is_empty()));
    assert!(live.iter().any(|peer| peer.storage.is_none()));
    assert!(live
        .iter()
        .any(|peer| !peer.compute.is_empty() && peer.storage.is_some()));

    let unauthorized = peer_capabilities("peer/unauthorized", true, false, digest);
    assert_eq!(
        directory.update(unauthorized, &AllowPeer { allow: false }, 1),
        Err(AiError::Unauthorized)
    );
    assert_eq!(
        directory.metrics().unwrap(),
        PeerDirectoryMetrics {
            current_peers: 3,
            compute_devices: 2,
            storage_peers: 2,
            donated_workers: 2,
            donated_storage_bytes: 8_192,
            accepted_updates: 3,
            rejected_updates: 1,
            expired_peers: 0,
        }
    );
    assert!(AiNodeCapabilities::from_contribution(
        PeerId::new("peer/disabled").unwrap(),
        CapabilityId::new("tenant/test").unwrap(),
        0,
        100,
        vec![AdvertisedCompute {
            backend: BackendId::new("remote/linear").unwrap(),
            device: DeviceId::new("peer/disabled/cpu").unwrap(),
            kind: DeviceKind::Cpu,
            metrics: PlacementMetrics::default(),
        }],
        None,
        BTreeSet::new(),
        donated_budget(25, 0),
        contribution_policy(false, false),
    )
    .is_err());
    assert!(directory
        .live(&CapabilityId::new("tenant/test").unwrap(), 1_000, 3)
        .unwrap()
        .is_empty());
    assert_eq!(directory.metrics().unwrap().expired_peers, 3);
}

#[cfg(feature = "swarm")]
#[test]
fn peer_directory_rejects_stale_and_internally_inconsistent_claims() {
    let digest = ArtifactDigest::from_bytes(b"peer-model");
    let directory = PeerCapabilityDirectory::new(PeerDirectoryConfig::default()).unwrap();
    let authorizer = AllowPeer { allow: true };
    let current = peer_capabilities("peer/stale", true, true, digest);
    directory.update(current.clone(), &authorizer, 1).unwrap();
    assert_eq!(
        directory.update(current.clone(), &authorizer, 1),
        Err(AiError::Conflict("stale AI peer advertisement"))
    );

    let mut duplicate = peer_capabilities("peer/duplicate", true, false, digest);
    duplicate.compute.push(duplicate.compute[0].clone());
    assert_eq!(
        directory.update(duplicate, &authorizer, 1),
        Err(AiError::InvalidInput("AI peer advertisement"))
    );

    let mut oversized_storage = peer_capabilities("peer/storage-claim", false, true, digest);
    oversized_storage.storage.as_mut().unwrap().available_bytes = 4_097;
    assert_eq!(
        directory.update(oversized_storage, &authorizer, 1),
        Err(AiError::InvalidInput("AI peer advertisement"))
    );
}

#[cfg(feature = "swarm")]
#[test]
fn peer_directory_handles_one_thousand_peer_churn_with_bounded_pruning() {
    let digest = ArtifactDigest::from_bytes(b"peer-model");
    let directory = PeerCapabilityDirectory::new(PeerDirectoryConfig {
        max_peers: 1_000,
        max_devices_per_peer: 1,
        max_artifacts_per_peer: 1,
        max_ttl: Duration::from_secs(1),
    })
    .unwrap();
    let authorizer = AllowPeer { allow: true };
    for index in 0..1_000 {
        directory
            .update(
                peer_capabilities(&format!("peer/churn-{index}"), true, false, digest),
                &authorizer,
                1,
            )
            .unwrap();
    }
    let tenant = CapabilityId::new("tenant/test").unwrap();
    assert_eq!(directory.live(&tenant, 2, 1_000).unwrap().len(), 1_000);
    assert!(directory.live(&tenant, 1_001, 1_000).unwrap().is_empty());
    let metrics = directory.metrics().unwrap();
    assert_eq!(metrics.current_peers, 0);
    assert_eq!(metrics.accepted_updates, 1_000);
    assert_eq!(metrics.expired_peers, 1_000);
}

#[cfg(feature = "swarm")]
#[derive(Debug)]
struct FakeSwarm {
    routes: Vec<SwarmRoute>,
    route_queries: AtomicUsize,
    executions: AtomicUsize,
}

#[cfg(feature = "swarm")]
impl SwarmBridge for FakeSwarm {
    fn routes(
        &self,
        _request: &AiRequest,
        _model: &ModelDescriptor,
        max_peers: usize,
    ) -> AiResult<Vec<SwarmRoute>> {
        self.route_queries.fetch_add(1, Ordering::Relaxed);
        Ok(self.routes.iter().take(max_peers).cloned().collect())
    }

    fn execute<'a>(
        &'a self,
        route: &'a SwarmRoute,
        _request: &'a AiRequest,
        cancellation: &'a CancellationToken,
    ) -> BackendFuture<'a, AiResponse> {
        Box::pin(async move {
            if cancellation.is_cancelled() {
                return Err(AiError::Cancelled);
            }
            self.executions.fetch_add(1, Ordering::Relaxed);
            if route.peer.as_str() == "peer/a-fail" {
                return Err(AiError::BackendUnavailable(route.backend.clone()));
            }
            AiResponse::new(
                AiOutput::Text("remote-answer".into()),
                Vec::new(),
                None,
                AiLimits::default(),
            )
        })
    }
}

#[cfg(feature = "swarm")]
fn swarm_route(peer: &str, backend: &str, artifact_peer: &str) -> SwarmRoute {
    SwarmRoute {
        peer: PeerId::new(peer).unwrap(),
        peer_class: "trusted-worker".into(),
        tenant: CapabilityId::new("tenant/test").unwrap(),
        backend: BackendId::new(backend).unwrap(),
        device: DeviceId::new(format!("{peer}/cpu")).unwrap(),
        kind: DeviceKind::Cpu,
        health: BackendHealth::Healthy,
        metrics: PlacementMetrics {
            load_percent: Some(10),
            queue_depth: 0,
            available_memory_bytes: Some(4_096),
            available_vram_bytes: Some(0),
            latency_ema_ms: Some(10),
            throughput_ema: Some(10),
        },
        resources: ResourceEstimate {
            cpu_percent: 20,
            memory_bytes: 1_024,
            workers: 1,
            ..ResourceEstimate::default()
        },
        artifact_source: Some(ArtifactLocation::Peer(PeerId::new(artifact_peer).unwrap())),
        model_resident: false,
        load_time_ms: 10,
        transfer_cost_units: 20,
        inference_cost_units: 10,
        rtt_ms: 5,
        bandwidth_bytes_per_second: Some(1_000_000),
        failover_cost_units: 5,
        lease_remaining: Duration::from_secs(5),
    }
}

#[cfg(feature = "swarm")]
#[test]
fn resolve_swarm_fails_over_and_auto_can_prefer_local() {
    let limits = AiLimits::default();
    let mut descriptor = model_descriptor("model/swarm", b"swarm");
    descriptor.supported_backends = vec![
        BackendId::new("remote/a").unwrap(),
        BackendId::new("remote/b").unwrap(),
    ];
    let models = Arc::new(ModelRegistry::new());
    models
        .register(
            descriptor.clone(),
            [ArtifactLocation::Peer(PeerId::new("peer/storage").unwrap())],
        )
        .unwrap();
    let bridge = Arc::new(FakeSwarm {
        routes: vec![
            swarm_route("peer/a-fail", "remote/a", "peer/storage"),
            swarm_route("peer/b", "remote/b", "peer/storage"),
        ],
        route_queries: AtomicUsize::new(0),
        executions: AtomicUsize::new(0),
    });
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000).unwrap()),
        models,
        Arc::new(BackendRegistry::new()),
        Arc::new(AlwaysAdmit),
    )
    .unwrap()
    .with_swarm_bridge(bridge.clone());
    let mut request = AiRequest::text(AiTask::GenerateText, "answer", limits).unwrap();
    request.options.execution = AiExecutionMode::Swarm;
    request.options.distribution.allow_remote_compute = true;
    request.options.distribution.allow_remote_storage = true;
    request.options.authorization = Some(remote_authorization(true));
    request.options.include_diagnostics = true;
    let response = block_on(runtime.resolve(request)).unwrap();
    assert!(matches!(
        response.decision,
        Some(ExecutionDecision {
            selected: ExecutionTarget::Remote { .. },
            attempts,
            ..
        }) if attempts.len() == 2
    ));
    assert_eq!(bridge.executions.load(Ordering::Relaxed), 2);

    descriptor
        .supported_backends
        .push(BackendId::new("native").unwrap());
    let local_models = Arc::new(ModelRegistry::new());
    local_models
        .register(descriptor, [ArtifactLocation::LocalStorage])
        .unwrap();
    let local_backends = Arc::new(BackendRegistry::new());
    local_backends
        .register(Arc::new(FakeBackend::new("native", false)))
        .unwrap();
    let mut expensive_remote = swarm_route("peer/b", "remote/b", "peer/storage");
    expensive_remote.rtt_ms = 200;
    expensive_remote.transfer_cost_units = 1_000;
    expensive_remote.metrics.load_percent = Some(90);
    expensive_remote.metrics.queue_depth = 10;
    let local_bridge = Arc::new(FakeSwarm {
        routes: vec![expensive_remote],
        route_queries: AtomicUsize::new(0),
        executions: AtomicUsize::new(0),
    });
    let auto = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000).unwrap()),
        local_models,
        local_backends,
        Arc::new(AlwaysAdmit),
    )
    .unwrap()
    .with_swarm_bridge(local_bridge.clone());
    let mut auto_request = AiRequest::text(AiTask::GenerateText, "answer", limits).unwrap();
    auto_request.options.distribution.allow_remote_compute = true;
    auto_request.options.authorization = Some(remote_authorization(false));
    assert_eq!(
        block_on(auto.resolve(auto_request)).unwrap().output,
        AiOutput::Text("backend-answer".into())
    );
    assert_eq!(local_bridge.executions.load(Ordering::Relaxed), 0);
}
