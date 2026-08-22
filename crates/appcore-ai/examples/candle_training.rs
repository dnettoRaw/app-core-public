// =============================================================================
//        #######
//     ###       ###     F: candle_training.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use appcore_ai::{
    AiContributionPolicy, AiResourceLimits, AiResourceMode, ArtifactStore, CancellationToken,
    CandleTrainer, CandleTrainerConfig, CapabilityId, GovernorTrainingAdmission,
    InMemoryTrainingDataset, MemoryArtifactStore, ModelId, ResourceEstimate, ResourceGovernor,
    ResourceGovernorConfig, SystemAiClock, SystemHardwareProbe, TrainingBackend,
    TrainingCheckpointPolicy, TrainingDataset, TrainingExample, TrainingJob, TrainingProgress,
    TrainingProgressObserver,
};
use std::future::Future;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

#[derive(Debug)]
struct ProgressPrinter;

impl TrainingProgressObserver for ProgressPrinter {
    fn report(&self, progress: &TrainingProgress) {
        if progress.checkpoint.is_some() {
            println!(
                "checkpoint epoch={} step={} loss={:.4}",
                progress.epoch, progress.step, progress.loss
            );
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let memory = Arc::new(MemoryArtifactStore::new(4 * 1024 * 1024)?);
    let store: Arc<dyn ArtifactStore> = memory;
    let governor = ResourceGovernor::new(
        SystemHardwareProbe::default(),
        ResourceGovernorConfig::default(),
        AiContributionPolicy::default(),
    )?;
    let admission = GovernorTrainingAdmission::new(governor, SystemAiClock::new());
    let trainer = CandleTrainer::new(
        Arc::clone(&store),
        Arc::new(admission),
        CandleTrainerConfig::default(),
    )?;

    let dataset: Arc<dyn TrainingDataset> = Arc::new(InMemoryTrainingDataset::new(
        vec![
            TrainingExample {
                text: "a".into(),
                label: 0,
            },
            TrainingExample {
                text: "aa".into(),
                label: 0,
            },
            TrainingExample {
                text: "b".into(),
                label: 1,
            },
            TrainingExample {
                text: "bb".into(),
                label: 1,
            },
        ],
        16,
        32,
    )?);
    let mode = AiResourceMode::Custom(AiResourceLimits {
        max_cpu_percent: 80,
        max_memory_bytes: 16 * 1024 * 1024,
        max_vram_bytes: 0,
        max_workers: 1,
        max_concurrent_jobs: 1,
    });
    let job = TrainingJob {
        id: CapabilityId::new("job/example-linear")?,
        model: ModelId::new("example/trained-linear")?,
        revision: "v1".into(),
        labels: vec!["class-a".into(), "class-b".into()],
        input_dimensions: 256,
        epochs: 4,
        max_steps: 8,
        batch_size: 4,
        learning_rate: 0.5,
        seed: 42,
        resource_requirements: ResourceEstimate {
            cpu_percent: 60,
            memory_bytes: 2 * 1024 * 1024,
            workers: 1,
            ..ResourceEstimate::default()
        },
        resource_mode: mode,
        checkpoints: TrainingCheckpointPolicy {
            every_epochs: 2,
            max_checkpoints: 2,
        },
        resume: None,
        publisher: None,
        max_input_bytes: 32,
        max_output_bytes: 1_024,
    };

    let output = block_on(trainer.train(
        &job,
        dataset,
        Arc::new(ProgressPrinter),
        &CancellationToken::new(),
    ))?;
    let stored = store.load(
        &output.identity,
        u64::try_from(output.bytes.len())?,
        &CancellationToken::new(),
    )?;
    println!(
        "epochs={} steps={} final_loss={:.4} artifact_bytes={} stored={}",
        output.completed_epochs,
        output.completed_steps,
        output.final_loss,
        output.bytes.len(),
        stored == output.bytes
    );
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
