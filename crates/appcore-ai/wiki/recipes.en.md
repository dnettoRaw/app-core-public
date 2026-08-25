# Concrete appcore-ai recipes

[Português](recipes.pt.md) | [Français](recipes.fr.md) |
[Guide](guide.en.md) | [Basic example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

This page uses APIs that exist in `0.1.0-beta.2`. It assumes no V1 manifest
field or hidden backend. Explicit host composition is available through
`appcore-bin/ai-alpha`; declarative selection awaits a post-1.0 contract.

## Quick choice

| Need | Feature | Starting point |
|---|---|---|
| normalize or classify by rule | none | [`lightweight_runtime.rs`](../examples/lightweight_runtime.rs) |
| CPU linear inference through runtime | `backend-candle` | [`candle_runtime.rs`](../examples/candle_runtime.rs) |
| call the Candle SPI only | `backend-candle` | [`candle_cpu.rs`](../examples/candle_cpu.rs) |
| train and write checkpoints | `training-candle` | [`candle_training.rs`](../examples/candle_training.rs) |
| bridge to AppCore peers | `swarm` | host-implemented `SwarmBridge` |
| local/private generative text, tools or vision | `backend-openai-compatible` | [`openai_compatible.rs`](../examples/openai_compatible.rs) |

## Separate local and contribution budgets

`AiContributionPolicy` never expands the local budget. This example retains up
to 70% CPU and 64 MiB for local work but advertises at most 25% CPU, 8 MiB RAM,
two workers, and 512 MiB artifact storage to authorized peers:

```rust
use appcore_ai::{
    AiContributionPolicy, AiResourceLimits, AiResourceMode, AiResult,
    ResourceGovernor, ResourceGovernorConfig, SystemHardwareProbe,
};

fn budgets() -> AiResult<()> {
    let contribution = AiContributionPolicy {
        contribute_compute: true,
        contribute_storage: true,
        max_cpu_percent: 25,
        max_gpu_percent: 0,
        max_memory_bytes: 8 * 1024 * 1024,
        max_vram_bytes: 0,
        max_storage_bytes: 512 * 1024 * 1024,
        max_workers: 2,
        max_concurrent_jobs: 1,
    };
    let governor = ResourceGovernor::new(
        SystemHardwareProbe::default(),
        ResourceGovernorConfig::default(),
        contribution,
    )?;
    let mode = AiResourceMode::Custom(AiResourceLimits {
        max_cpu_percent: 70,
        max_memory_bytes: 64 * 1024 * 1024,
        max_vram_bytes: 0,
        max_workers: 4,
        max_concurrent_jobs: 2,
    });
    let pair = governor.budgets(mode, 0)?;
    assert_eq!(pair.local.cpu_percent, 70);
    assert_eq!(pair.contribution.cpu_percent, 25);
    assert_eq!(pair.contribution.memory_bytes, Some(8 * 1024 * 1024));
    assert_eq!(pair.contribution.storage_bytes, 512 * 1024 * 1024);
    Ok(())
}
```

For a strictly local node, use `AiContributionPolicy::default()`. Donated
budgets remain zero even when local mode is `Performance` or `Unrestricted`.

## SHA-256 local cache with atomic activation

`LocalArtifactCache` derives the filename from the digest; external names never
select the final path. The store validates digest and size before activation:

```rust
use appcore_ai::{ArtifactDigest, ArtifactIdentity, LocalArtifactCache};

let bytes = b"bounded-model-bytes";
let identity = ArtifactIdentity {
    digest: ArtifactDigest::from_bytes(bytes),
    size_bytes: u64::try_from(bytes.len())?,
    publisher: None,
    signature_required: false,
};
let root = std::env::temp_dir().join(format!(
    "appcore-ai-cache-example-{}",
    std::process::id()
));
let cache = LocalArtifactCache::new(&root, 1024)?;
let path = cache.store(&identity, bytes)?;
assert_eq!(cache.load(&identity)?, bytes);
assert_eq!(path, cache.path(identity.digest));
std::fs::remove_dir_all(root)?;
```

Changing `bytes` after identity creation makes `store` fail with
`AiError::Integrity("artifact digest")`. For mandatory signatures, wrap an
`ArtifactStore` in `ProvenanceArtifactStore`; its verifier adapts AppCore
security and does not keep a private key inside this crate.

## Cooperative cancellation and deadline

The caller owns the token and may cancel all clones. The runtime checks it
before routing, loading, and inference; backends must cooperate as well:

```rust
let cancellation = appcore_ai::CancellationToken::new();
let mut request = appcore_ai::AiRequest::text(
    appcore_ai::AiTask::TransformText,
    "bounded input",
    limits,
)?;
request.options.execution = appcore_ai::AiExecutionMode::Local;
request.options.deadline = Some(std::time::Duration::from_millis(250));

cancellation.cancel();
let result = runtime
    .resolve_with_cancellation(request, cancellation)
    .await;
assert_eq!(result, Err(appcore_ai::AiError::Cancelled));
```

The deadline is relative to the start of `resolve`; it neither kills a thread
nor interrupts a blocking backend. Backend adapters must split long work and
inspect the token.

## Unambiguous Local, Auto, and Swarm modes

| Mode | Remote compute | Remote storage | Bridge required |
|---|---:|---:|---:|
| `Local` | never | only when explicitly allowed and not `LocalOnly` | no |
| `Auto` | only with grant and policy | only with grant and policy | for remote candidates |
| `Swarm` | required | optional and independent | yes |

A request requiring remote compute must declare both policy and grant:

```rust
use appcore_ai::{
    AiAuthorizationContext, AiExecutionMode, AiPrivacyMode, CapabilityId,
    REMOTE_COMPUTE_GRANT,
};

request.options.execution = AiExecutionMode::Swarm;
request.options.privacy = AiPrivacyMode::TrustedSwarm;
request.options.distribution.allow_remote_compute = true;
request.options.distribution.allow_remote_storage = false;
request.options.authorization = Some(AiAuthorizationContext {
    tenant: CapabilityId::new("tenant/example")?,
    subject: CapabilityId::new("subject/example")?,
    grants: vec![CapabilityId::new(REMOTE_COMPUTE_GRANT)?],
});
```

Without `runtime.with_swarm_bridge(...)`, the result is
`AiError::SwarmUnavailable`. Remote storage also requires
`REMOTE_STORAGE_GRANT`. Combining `LocalOnly` with any remote permission is
invalid input. The bridge must reuse AppCore authentication, discovery, replay,
and Peer RPC.

## Local reproducible Candle training

Run the complete job:

```bash
cargo run -p appcore-ai --example candle_training --features training-candle
```

Deterministic output for the included dataset:

```text
checkpoint epoch=2 step=4 loss=0.6634
checkpoint epoch=4 step=8 loss=0.3914
epochs=4 steps=8 final_loss=0.3914 artifact_bytes=2090 stored=true
```

The program explicitly configures dataset, seed, epochs, steps, batch,
resources, and checkpoint frequency. `TrainingOutput` includes bytes, identity,
and a registry-ready `ModelDescriptor`:

```rust
let output = trainer
    .train(&job, dataset, progress, &cancellation)
    .await?;
models.register(
    output.descriptor.clone(),
    [appcore_ai::ArtifactLocation::Memory],
)?;
```

Use the same `ArtifactStore` for `CandleTrainer` and `CandleBackend`; the trainer
has already stored the final artifact. Assign a verified identity to
`job.resume` to resume. Distributed training is unsupported.

## Redacted observations and metrics

Connect `AiObservationSink` to the composition root's `appcore-ops` adapter.
Events contain no prompt, output, model ID, peer ID, or credential:

```rust
use appcore_ai::{AiObservation, AiObservationSink};

struct OpsAdapter;

impl AiObservationSink for OpsAdapter {
    fn record(&self, observation: &AiObservation) {
        match observation {
            AiObservation::RequestCompleted { success, attempts, .. } => {
                record_counter("ai.request.completed", *success, *attempts);
            }
            _ => record_event_class(observation),
        }
    }
}

let runtime = runtime.with_observation_sink(std::sync::Arc::new(OpsAdapter));
```

`record_counter` and `record_event_class` are host adapter functions, not crate
APIs. For local polling, use `runtime.telemetry()` and publish aggregate fields
only.

## Backpressure before the backend

Use one `FairQueue` per dispatch domain and reject overload structurally:

```rust
use appcore_ai::{
    AiPriority, CancellationToken, FairQueue, FairQueueConfig, QueueAdmission,
};
use std::time::Duration;

let mut queue = FairQueue::new(FairQueueConfig {
    capacity: 2,
    starvation_after: Duration::from_secs(1),
    overload_retry_after: Duration::from_millis(25),
})?;
assert!(matches!(
    queue.enqueue("one", AiPriority::Normal, 0, None, CancellationToken::new()),
    QueueAdmission::Queued { .. }
));
queue.enqueue("two", AiPriority::High, 0, None, CancellationToken::new());
let third = queue.enqueue(
    "three",
    AiPriority::Normal,
    0,
    None,
    CancellationToken::new(),
);
assert!(matches!(third, QueueAdmission::Rejected { .. }));
```

Partition `DynamicBatcher` by the complete `BatchKey`: model, backend, device,
and task class must match. Never batch requests merely because their inputs
share a type.

## Quick diagnosis

| Error | Check first |
|---|---|
| `NotFound("compatible AI route")` | task, model ID, state, location, backend, and device |
| `Capacity("all model routes were denied")` | `ResourceEstimate`, mode, known RAM/VRAM, and pressure |
| `Unauthorized` | tenant, separate compute/storage grants, and privacy |
| `SwarmUnavailable` | `swarm` feature, composed bridge, and live advertisements |
| `Integrity` | digest, size, publisher, signature, and validity window |
| `BackendUnavailable` | model lifecycle and backend health |
| `LimitExceeded` | named limit, actual size, and attempts/peers/batch |

Errors are part of the contract. Do not add silent fallback, enable
`Unrestricted` automatically, or turn unknown capacity into infinite capacity.
