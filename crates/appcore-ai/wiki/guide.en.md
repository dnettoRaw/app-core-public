# appcore-ai guide

[Português](guide.pt.md) | [Français](guide.fr.md) |
[Basic example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md) |
[Concrete recipes](recipes.en.md) |
[Models and training](models.en.md) |
[Generative LLMs](generative-llm.en.md) |
[Hardware resources](resources.en.md) |
[Architecture ADR](architecture-adr.en.md) |
[Threat model](threat-model.en.md) |
[Release readiness](release-readiness.en.md)

`appcore-ai` owns product-independent, bounded AI orchestration. It does not
own application prompts, business schemas, provider credentials or workflows.
It has independent SemVer; this release is `0.1.0-beta.1`.

## Learning path

1. Run the [lightweight runtime](examples/basic.en.md) without optional features.
2. Run [Candle inference through AiRuntime](examples/intermediate.en.md).
3. Configure or train a classifier in [models and training](models.en.md).
4. Read the [adaptive runtime profile](generative-llm.en.md) for text, vision,
   PDF, multi-engine selection, owned residency, and recommended models.
5. Use the [concrete recipes](recipes.en.md) for resources, cache, cancellation,
   Swarm, training, observability, and backpressure.
6. Before production, read the [threat model](threat-model.en.md) and
   [release readiness](release-readiness.en.md).

The `lightweight_runtime`, `candle_runtime`, `openai_compatible`, and
`candle_training` examples are
compiled sources under `examples/`; the main wiki snippets are derived from
them and include commands and expected output.

## Architecture and resolution

```text
AiRuntime::resolve
  -> validate modality, content, privacy, authorization and bounds
  -> deterministic lightweight resolver
  -> apply the Fast/Balanced/Deep/Maximum quality floor
  -> model registry and artifact identity
  -> ResourceGovernor admission
  -> cost scheduler (CPU/GPU/NPU/authorized peer)
  -> bounded fair execution admission
  -> backend `infer` or explicitly coordinated compatible `infer_batch`
  -> residency planner (VRAM -> RAM -> local -> peer)
  -> backend or SwarmBridge
  -> bounded escalation
  -> redacted response diagnostics and telemetry
```

The scheduler scores current load, headroom, queue depth, model residency,
activation/transfer cost, latency/throughput EMA, priority, deadline and
resource mode. Integer weights and injected clocks make tests deterministic.
Compute placement and artifact placement remain separate.

The lightweight engine performs bounded normalization and explicit exact,
prefix or contains rules. A rule reports its reason and certainty. It can
return immediately or retain a safe fallback while the router escalates.

## Resources and modes

`ResourceGovernor` samples through `HardwareProbe`, caches observations and
uses hysteresis. Unknown RAM/VRAM is not guessed. Local budgets and donated
budgets are distinct, and `AiContributionPolicy` can independently disable
compute or storage contribution.

`SystemHardwareProbe::default()` reads real CPU/RAM signals on macOS, Linux
and Windows. It discovers Apple unified GPUs, Linux DRM devices and, behind
`accelerator-nvidia`, NVIDIA VRAM/utilization through NVML. Exact-device fit
prevents aggregate multi-GPU overcommit. See the
[hardware resource guide](resources.en.md) for the platform matrix, executable
report, dependency cost and operational semantics.

| Mode | Voluntary AppCore policy |
|---|---|
| `Eco` | maximum host headroom, smallest batches |
| `Balanced` | interactive host headroom, conservative training batches |
| `Performance` | favors throughput with a safety margin |
| `Unrestricted` | removes voluntary AppCore headroom within backend/OS limits |
| `Custom` | caller-specified validated ceilings |

`Unrestricted` never disables OS, driver, firmware, thermal or electrical
protections. It cannot promise that hardware will not throttle.

Queues, batches, attempts, peers, artifacts, transfers, inputs, outputs,
metadata, workers and concurrent jobs all have explicit bounds. Cancellation
and deadlines are checked before dispatch and between cooperative phases.

## Models, artifacts and residency

`ModelRegistry` keeps immutable metadata separate from lifecycle state and
artifact locations. `ArtifactIdentity` is SHA-256 plus exact size and optional
publisher provenance. Local cache writes use exclusive temporary files, sync
and atomic activation. Peer filenames are never trusted.

```text
ArtifactIdentity
  +-> Vram(device)
  +-> Memory
  +-> LocalStorage
  +-> Peer(peer)       (bytes reverified before promotion)
```

`ResidencyPlanner` implements initial LRU-style reuse, safe two-phase eviction,
bounded prefetch, fallback tiers and rollback after failed loads. A concurrent
request sees `InFlight` instead of loading the same target twice.

## Optional backend and training

The default feature set contains no ML framework. `backend-candle` enables one
real CPU backend for the data-only `NativeLinearV1` classifier format. It
supports verified load, unload, inference, batching fallback, cancellation,
metrics and thread-safe concurrent inference. It never downloads a model.

```bash
cargo run -p appcore-ai --example candle_cpu --features backend-candle
```

`training-candle` adds local SGD for the same format. Jobs bound examples,
dimensions, labels, epochs, steps, batch, resources and checkpoints. Seeds are
reproducible; checkpoints use `ArtifactStore` atomic activation and can resume.
Distributed training is unsupported.

`backend-openai-compatible` is the real generative path for a separately
running llama.cpp, MLX-LM, TabbyAPI, vLLM, SGLang, TensorRT-LLM, OpenVINO or
tested compatible server. It supports role-aware chat, bounded sampling,
tools/tool calls and explicitly declared image input. The default transport is
loopback-first and unauthenticated; remote credentials require an AppCore
security transport adapter.

## Local, Swarm and Auto

The `swarm` feature is experimental and requires an authenticated
`SwarmBridge` supplied by the AppCore composition root.

```text
storage-only node -> ArtifactStore(peer) ----+
compute-only node -> ComputeTarget(peer) ----+-> Auto planner -> execute
combined node     -> both -------------------+
local node        -> CPU/GPU/NPU + cache ----+
```

- `Local` never queries a peer.
- `Swarm` requires an authorized remote route and fails closed otherwise.
- `Auto` compares permitted local and remote cost; local privacy policy wins.

Advertisements contain only the budget left after local contribution policy,
expire, and are authenticated by an adapter to existing AppCore security.
Remote compute requires the `ai.remote.compute` tenant grant; peer storage
requires `ai.remote.storage`. Artifact transport is separate from generic Peer
RPC, so large model bytes are not command payloads. Peer disappearance or
backend failure triggers bounded failover. Remote result correctness is not
cryptographically provable in general; peer/result trust remains explicit.

## Security and observability

`ModelSecurityPolicy` rejects provider/custom-op formats by default and caps
artifact/RAM/VRAM metadata. `ProvenanceArtifactStore` delegates cryptographic
signature verification to AppCore security rather than reimplementing crypto.
`Debug` output redacts prompts, binary content, generated text, embeddings,
classifier labels and metadata values. Credential fields are references only.

`AiTelemetry` exposes fixed-bucket p50/p95/p99, request outcomes, admissions,
loads, fallback/escalation and local/lightweight/remote placement. Events have
only bounded enums and no model, backend, device, peer, tenant or payload IDs.
`AiObservationSink` is the adapter point for `appcore-ops`.

Component snapshots complete the bounded operational view: `FairQueueMetrics`
and `BatcherMetrics` expose depth, saturation and batch items;
`ResidencyMetrics` exposes reuse, pending loads, rollbacks, evictions and
resident bytes; `PeerArtifactMetrics` exposes verified remote fetch bytes;
`PeerDirectoryMetrics` exposes aggregate availability, contribution and churn;
backend placement metrics expose queue/pressure/throughput, and training uses a
bounded progress observer. The composition adapter maps these aggregates to
`appcore-ops` without arbitrary-ID labels.

`AiRuntime::model_loads()` exposes ready/loading gauges plus ready-hit, waiter,
loader, eviction and invalidation counters. Use it to detect repeated cold
loads or a route left in loading state; it contains no model/backend IDs.

## Public API levels

The flat crate exports are grouped by intended use, not by stability promises:

| Level | Typical types | Intended caller |
|---|---|---|
| Essential | `AiRuntime`, `AiRequest`, `AiResponse`, `AiOutput`, `AiOptions`, `AiLimits`, cancellation and errors | applications resolving bounded AI work |
| Advanced policy | governor/admission, registries, scheduler, queues, batching, residency, artifacts, bundles, telemetry and security types | a composition root tuning placement and resources |
| Backend SPI | `InferenceBackend`, descriptors/futures, `ArtifactStore`, peer transport, observations, planners, optional training and OpenAI transport traits | backend/provider and host adapters |
| Internal | route construction, load permits, execution queue, scoring and HTTP codecs | crate implementation; deliberately not exported |

The default dependency graph contains no ML or HTTP engine. `sha2` supplies
artifact identity; the target-specific `libc` or `windows-sys` edge supplies
safe no-follow file flags and native resource counters. `nvml-wrapper` is
isolated behind `accelerator-nvidia`; Candle and OpenAI-compatible dependencies
remain behind explicit features. `#![deny(unsafe_code)]` applies to the crate;
narrowly scoped, documented native FFI is allowed only inside the macOS and
Windows
resource modules. Linux discovery and the optional NVIDIA wrapper use safe APIs.

## Performance and load evidence

`perf_lab` covers lightweight/missing/cold/warm resolution, 1/32/128 registry
and scheduler scaling, 1/2/4/8/16 batching, local artifact full/range reads,
Candle 1/8/32 batches and training, and Swarm 1/10/100/1,000 planning. Emit
machine-readable results with:

```bash
APPCORE_AI_BENCH_FORMAT=jsonl \
  cargo bench -p appcore-ai --bench perf_lab --all-features
APPCORE_AI_SOAK_ITERATIONS=100000 \
  cargo test -p appcore-ai --test stress_soak --all-features -- --nocapture
```

See the [optimization report](benchmarks.en.md) for the measured before/after,
memory methodology, intentional artifact-security cost and interpretation
limits.

## Usage patterns

1. Default lightweight resolution:

   ```rust
   async fn normalize(ai: &appcore_ai::AiRuntime) -> appcore_ai::AiResult<()> {
       let request = appcore_ai::AiRequest::text(
           appcore_ai::AiTask::TransformText,
           "  bounded   text ",
           appcore_ai::AiLimits::default(),
       )?;
       let _response = ai.resolve(request).await?;
       Ok(())
   }
   ```

2. Forced local model: set `execution = Local` and `options.model = Some(id)`;
   resolution fails explicitly if that model/backend/device cannot be admitted.
3. Custom resources: use `AiResourceMode::Custom(AiResourceLimits { .. })`.
4. Maximum voluntary throughput: use `Unrestricted` only after accepting the
   host-pressure and throttling warning above.
5. Optional classifier inference: run `examples/candle_cpu.rs` with `backend-candle`.
6. Optional generative inference: configure and run `examples/openai_compatible.rs`.
7. Optional training: create `TrainingJob` and `TrainingDataset`, then call a
   `CandleTrainer` configured with mandatory `TrainingAdmission`.

## Limits and gates

There is no AppCore V1 manifest field or CLI flag. The opt-in
`appcore-bin/ai-alpha` feature adds `ManifestApplicationHost::with_ai`, an
`ApplicationAi` facade, an `appcore.ai.resolve` capability handler and graceful
Supervisor lifecycle without changing V1. Declarative deployment selection
still requires an accepted post-1.0 contract. Swarm transport, authentication,
replay storage and process isolation remain host/deployment responsibilities;
the crate does not claim a sandbox or zero trust.

Run the beta evidence:

```bash
cargo test -p appcore-ai --all-targets --all-features
./crates/appcore-ai/scripts/check-feature-matrix.sh
cargo bench -p appcore-ai --bench perf_lab --all-features
```
