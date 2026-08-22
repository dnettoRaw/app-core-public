// =============================================================================
//        #######
//     ###       ###     F: candle_cpu.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::{
    AiLimits, AiModality, AiOutput, AiRequest, AiTask, ArtifactFormat, ArtifactStore, BackendId,
    CancellationToken, CandleBackend, CandleBackendConfig, DeviceId, DeviceKind, InferenceBackend,
    MemoryArtifactStore, ModelDescriptor, ModelId, NativeLinearArtifact, QualityTier, Quantization,
    CANDLE_LINEAR_BACKEND_ID,
};
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

fn main() -> Result<(), Box<dyn std::error::Error>> {
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
        id: ModelId::new("example/candle-linear")?,
        revision: "v1".into(),
        tasks: vec![AiTask::ClassifyText],
        input_modalities: vec![AiModality::Text],
        format: ArtifactFormat::NativeLinearV1,
        quantization: Quantization::None,
        estimated_memory_bytes: u64::try_from(bytes.len() * 2)?,
        estimated_vram_bytes: 0,
        max_input_bytes: 1024,
        max_output_bytes: 1024,
        context_limit: None,
        supported_backends: vec![BackendId::new(CANDLE_LINEAR_BACKEND_ID)?],
        supported_devices: vec![DeviceKind::Cpu],
        load_cost_units: 10,
        quality: Some(QualityTier::Tiny),
        artifact: artifact.identity(None, false)?,
    };
    let memory = Arc::new(MemoryArtifactStore::new(1024 * 1024)?);
    memory.store(&descriptor.artifact, &bytes, &CancellationToken::new())?;
    let store: Arc<dyn ArtifactStore> = memory;
    let backend = CandleBackend::new(store, CandleBackendConfig::default())?;
    block_on(backend.load(&descriptor, &CancellationToken::new()))?;
    let request = AiRequest::text(AiTask::ClassifyText, "a", AiLimits::default())?;
    let response = block_on(backend.infer(
        &request,
        &descriptor,
        &DeviceId::new("local/cpu/candle")?,
        &CancellationToken::new(),
    ))?;
    if let AiOutput::Scores(scores) = response.output {
        println!("{}: {:.3}", scores[0].label, scores[0].score);
    }
    Ok(())
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
