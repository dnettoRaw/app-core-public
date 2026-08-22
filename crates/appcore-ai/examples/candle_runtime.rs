// =============================================================================
//        #######
//     ###       ###     F: candle_runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::{
    AiContributionPolicy, AiExecutionMode, AiLimits, AiModality, AiOutput, AiPrivacyMode,
    AiRequest, AiResourceLimits, AiResourceMode, AiRuntime, AiTask, ArtifactFormat,
    ArtifactLocation, ArtifactStore, BackendId, BackendRegistry, CancellationToken, CandleBackend,
    CandleBackendConfig, DeviceKind, GovernorAdmission, LightweightEngine, MemoryArtifactStore,
    ModelDescriptor, ModelId, ModelRegistry, NativeLinearArtifact, QualityTier, Quantization,
    ResourceGovernor, ResourceGovernorConfig, SystemAiClock, SystemHardwareProbe,
    CANDLE_LINEAR_BACKEND_ID,
};
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limits = AiLimits {
        max_input_bytes: 1_024,
        max_output_bytes: 1_024,
        ..AiLimits::default()
    };
    let (descriptor, bytes) = model_artifact()?;
    let model_id = descriptor.id.clone();

    let memory = Arc::new(MemoryArtifactStore::new(4 * 1024 * 1024)?);
    memory.store(&descriptor.artifact, &bytes, &CancellationToken::new())?;
    let store: Arc<dyn ArtifactStore> = memory;

    let backend = CandleBackend::new(store, CandleBackendConfig::default())?;
    let backends = Arc::new(BackendRegistry::new());
    backends.register(Arc::new(backend))?;

    let models = Arc::new(ModelRegistry::new());
    models.register(descriptor, [ArtifactLocation::Memory])?;

    let governor = ResourceGovernor::new(
        SystemHardwareProbe::default(),
        ResourceGovernorConfig::default(),
        AiContributionPolicy::default(),
    )?;
    let admission = GovernorAdmission::new(governor, SystemAiClock::new());
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 8_000)?),
        Arc::clone(&models),
        backends,
        Arc::new(admission),
    )?;

    let mut request = AiRequest::text(AiTask::ClassifyText, "a", limits)?;
    request.options.execution = AiExecutionMode::Local;
    request.options.privacy = AiPrivacyMode::LocalOnly;
    request.options.model = Some(model_id.clone());
    request.options.resources = AiResourceMode::Custom(AiResourceLimits {
        max_cpu_percent: 80,
        max_memory_bytes: 16 * 1024 * 1024,
        max_vram_bytes: 0,
        max_workers: 1,
        max_concurrent_jobs: 1,
    });
    request.options.include_diagnostics = true;

    let response = block_on(runtime.resolve(request))?;
    if let AiOutput::Scores(scores) = response.output {
        if let Some(best) = scores
            .iter()
            .max_by(|left, right| left.score.total_cmp(&right.score))
        {
            println!("class={} score={:.3}", best.label, best.score);
        }
    }
    if let Some(decision) = response.decision {
        println!("route={:?}", decision.selected);
    }
    println!("model_state={:?}", models.get(&model_id)?.state);
    let telemetry = runtime.telemetry();
    println!(
        "loads={} local_placements={} successes={}",
        telemetry.model_load_successes, telemetry.local_placements, telemetry.successes
    );
    Ok(())
}

fn model_artifact() -> Result<(ModelDescriptor, Vec<u8>), Box<dyn std::error::Error>> {
    let dimensions = 256;
    let mut weights = vec![0.0; dimensions * 2];
    weights[usize::from(b'a')] = 10.0;
    weights[dimensions + usize::from(b'b')] = 10.0;
    let artifact = NativeLinearArtifact::new(
        dimensions,
        vec!["class-a".into(), "class-b".into()],
        weights,
        vec![0.0, 0.0],
    )?;
    let bytes = artifact.encode()?;
    let descriptor = ModelDescriptor {
        id: ModelId::new("example/candle-runtime")?,
        revision: "v1".into(),
        tasks: vec![AiTask::ClassifyText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::NativeLinearV1,
        quantization: Quantization::None,
        estimated_memory_bytes: u64::try_from(bytes.len())?.saturating_mul(2),
        estimated_vram_bytes: 0,
        max_input_bytes: 1_024,
        max_output_bytes: 1_024,
        context_limit: None,
        supported_backends: vec![BackendId::new(CANDLE_LINEAR_BACKEND_ID)?],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 20,
        quality: Some(QualityTier::Tiny),
        artifact: artifact.identity(None, false)?,
    };
    Ok((descriptor, bytes))
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
