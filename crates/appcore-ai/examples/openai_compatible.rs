// =============================================================================
//        #######
//     ###       ###     F: openai_compatible.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::{
    AdmissionDecision, AdmissionReason, AiLimits, AiMessage, AiMessageRole, AiOutput, AiRequest,
    AiResult, AiRuntime, ArtifactDigest, ArtifactFormat, ArtifactIdentity, ArtifactLocation,
    BackendDevice, BackendId, BackendRegistry, CancellationToken, DeviceId, DeviceKind,
    LightweightEngine, ModelAdmission, ModelDescriptor, ModelId, ModelRegistry,
    OpenAiCompatibleBackend, OpenAiCompatibleConfig, OpenAiCompatibleEngine, QualityTier,
    Quantization, ResourceBudget, ResourceEstimate, SystemHardwareProbe,
    UnauthenticatedOpenAiHttpTransport,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::{Duration, Instant};

#[derive(Debug)]
struct ConfiguredAdmission {
    max_memory_bytes: u64,
}

impl ModelAdmission for ConfiguredAdmission {
    fn admit(
        &self,
        _request: &AiRequest,
        estimate: ResourceEstimate,
    ) -> AiResult<AdmissionDecision> {
        if estimate.memory_bytes > self.max_memory_bytes {
            return Ok(AdmissionDecision::Reject {
                reason: AdmissionReason::MemoryPressure,
            });
        }
        Ok(AdmissionDecision::Admit {
            budget: ResourceBudget {
                cpu_percent: 100,
                gpu_percent: 100,
                memory_bytes: Some(self.max_memory_bytes),
                vram_bytes: None,
                storage_bytes: 0,
                workers: 1,
                concurrent_jobs: 1,
                pressure_limited: false,
            },
        })
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (runtime, request) = configured_runtime()?;
    let benchmark_iterations = parse_env("APPCORE_AI_BENCH_ITERATIONS", 0)?;
    let probe = SystemHardwareProbe::with_sampling_interval(Duration::from_secs(1))?;
    let before = probe.refresh()?;
    let cold_started = Instant::now();
    let response =
        block_on(runtime.resolve_with_cancellation(request.clone(), CancellationToken::new()))?;
    let cold_latency = cold_started.elapsed();
    match response.output {
        AiOutput::Text(text) => println!("{text}"),
        _ => return Err("the configured server did not return text".into()),
    }
    if benchmark_iterations > 0 {
        if benchmark_iterations > 10_000 {
            return Err("APPCORE_AI_BENCH_ITERATIONS exceeds 10000".into());
        }
        let benchmark_iterations = usize::try_from(benchmark_iterations)?;
        let warm_started = Instant::now();
        let mut output_tokens = 0usize;
        for _ in 0..benchmark_iterations {
            let response = block_on(
                runtime.resolve_with_cancellation(request.clone(), CancellationToken::new()),
            )?;
            if let AiOutput::Text(text) = response.output {
                output_tokens = output_tokens.saturating_add(text.split_whitespace().count());
            }
        }
        let warm = warm_started.elapsed();
        let after = probe.refresh()?;
        println!("benchmark.cold_first_response={cold_latency:?}");
        println!("benchmark.first_token=unavailable_non_streaming_contract");
        println!("benchmark.server_model_load=external_to_chat_contract");
        println!(
            "benchmark.warm_average={:?}",
            warm / u32::try_from(benchmark_iterations).unwrap_or(u32::MAX)
        );
        println!(
            "benchmark.output_tokens_per_second={:.2}",
            output_tokens as f64 / warm.as_secs_f64()
        );
        println!(
            "benchmark.available_ram_before={:?} after={:?}",
            before.available_memory_bytes, after.available_memory_bytes
        );
        println!(
            "benchmark.cpu_after={:?}% devices={:?}",
            after.cpu_load_percent,
            after
                .devices
                .iter()
                .map(|device| (
                    device.kind,
                    device.utilization_percent,
                    device.available_memory_bytes
                ))
                .collect::<Vec<_>>()
        );
    }
    Ok(())
}

fn configured_runtime() -> Result<(AiRuntime, AiRequest), Box<dyn std::error::Error>> {
    let base_url = required_env("APPCORE_AI_BASE_URL")?;
    let server_model = required_env("APPCORE_AI_MODEL")?;
    let digest = ArtifactDigest::parse_hex(&required_env("APPCORE_AI_MODEL_SHA256")?)?;
    let model_bytes = parse_env("APPCORE_AI_MODEL_BYTES", 4_000_000_000)?;
    let memory_bytes = parse_env("APPCORE_AI_MEMORY_BYTES", 8_000_000_000)?;
    let backend_id = BackendId::new("local/openai-compatible")?;
    let model_id = ModelId::new("local/chat")?;
    let device = DeviceId::new("local/engine")?;

    let mut names = BTreeMap::new();
    names.insert(model_id.clone(), server_model);
    let config = OpenAiCompatibleConfig::local(
        engine_from_env()?,
        backend_id.clone(),
        base_url,
        vec![BackendDevice {
            id: device,
            kind: DeviceKind::Cpu,
        }],
        names,
    )?;
    let backend = Arc::new(OpenAiCompatibleBackend::new(
        config,
        Arc::new(UnauthenticatedOpenAiHttpTransport::default()),
    )?);
    let backends = Arc::new(BackendRegistry::new());
    backends.register(backend)?;

    let models = Arc::new(ModelRegistry::new());
    models.register(
        ModelDescriptor {
            id: model_id,
            revision: "configured".to_string(),
            tasks: vec![appcore_ai::AiTask::GenerateText, appcore_ai::AiTask::Chat],
            input_modalities: vec![appcore_ai::AiModality::Text],
            format: format_from_env()?,
            quantization: Quantization::Other(appcore_ai::CapabilityId::new("configured")?),
            estimated_memory_bytes: memory_bytes,
            estimated_vram_bytes: 0,
            max_input_bytes: 1_048_576,
            max_output_bytes: 1_048_576,
            context_limit: None,
            supported_backends: vec![backend_id],
            supported_devices: vec![DeviceKind::Cpu],
            load_cost_units: 10,
            quality: Some(QualityTier::Tiny),
            artifact: ArtifactIdentity {
                digest,
                size_bytes: model_bytes,
                publisher: None,
                signature_required: false,
            },
        },
        [ArtifactLocation::LocalStorage],
    )?;

    let limits = AiLimits::default();
    let runtime = AiRuntime::new(
        limits,
        Arc::new(LightweightEngine::new(Vec::new(), limits, 10_000)?),
        models,
        backends,
        Arc::new(ConfiguredAdmission {
            max_memory_bytes: memory_bytes.saturating_add(64 * 1_024 * 1_024),
        }),
    )?;
    let request = AiRequest::chat(
        [
            AiMessage::new(AiMessageRole::System, "Answer briefly and factually.")?,
            AiMessage::new(
                AiMessageRole::User,
                "Explain local-first AI in one sentence.",
            )?,
        ],
        limits,
    )?;
    Ok((runtime, request))
}

fn required_env(name: &str) -> Result<String, Box<dyn std::error::Error>> {
    std::env::var(name).map_err(|_| format!("missing required environment variable {name}").into())
}

fn parse_env(name: &str, default: u64) -> Result<u64, Box<dyn std::error::Error>> {
    std::env::var(name)
        .ok()
        .map(|value| value.parse::<u64>())
        .transpose()?
        .map_or(Ok(default), Ok)
}

fn engine_from_env() -> Result<OpenAiCompatibleEngine, Box<dyn std::error::Error>> {
    match std::env::var("APPCORE_AI_ENGINE")
        .as_deref()
        .unwrap_or("llama.cpp")
    {
        "llama.cpp" => Ok(OpenAiCompatibleEngine::LlamaCpp),
        "mlx-lm" => Ok(OpenAiCompatibleEngine::MlxLm),
        "vllm" => Ok(OpenAiCompatibleEngine::Vllm),
        "sglang" => Ok(OpenAiCompatibleEngine::Sglang),
        "tensorrt-llm" => Ok(OpenAiCompatibleEngine::TensorRtLlm),
        "openvino" => Ok(OpenAiCompatibleEngine::OpenVino),
        "tabbyapi" => Ok(OpenAiCompatibleEngine::TabbyApi),
        "generic" => Ok(OpenAiCompatibleEngine::Generic),
        value => Err(format!("unsupported APPCORE_AI_ENGINE: {value}").into()),
    }
}

fn format_from_env() -> Result<ArtifactFormat, Box<dyn std::error::Error>> {
    match std::env::var("APPCORE_AI_FORMAT")
        .as_deref()
        .unwrap_or("gguf")
    {
        "gguf" => Ok(ArtifactFormat::Gguf),
        "safetensors" => Ok(ArtifactFormat::SafeTensors),
        "onnx" => Ok(ArtifactFormat::Onnx),
        value => Err(format!("unsupported APPCORE_AI_FORMAT: {value}").into()),
    }
}

fn block_on<T>(future: impl Future<Output = T>) -> T {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = Box::pin(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(value) => return value,
            Poll::Pending => std::thread::yield_now(),
        }
    }
}
