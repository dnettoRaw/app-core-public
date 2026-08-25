# Local Candle inference through AiRuntime

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Basic example](basic.en.md) | [Recipes](../recipes.en.md) |
[Guide](../guide.en.md)

This example exercises the complete flow: create a data-only artifact, verify
and store its bytes, register a model and backend, apply resource admission,
load on demand, and classify on CPU through `AiRuntime`.

## Run it

```bash
cargo run -p appcore-ai --example candle_runtime --features backend-candle
```

Output:

```text
class=class-a score=1.000
route=Local { backend: BackendId("candle/cpu-linear-v1"), device: DeviceId("local/cpu/candle") }
model_state=Ready
loads=1 local_placements=1 successes=1
```

The complete compiled program is
[`examples/candle_runtime.rs`](../../examples/candle_runtime.rs). The
[`candle_cpu.rs`](../../examples/candle_cpu.rs) example calls the backend SPI
directly; applications should prefer the `AiRuntime` flow on this page.

## Dependency and feature

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.2", default-features = false, features = ["backend-candle"] }
```

The default build remains Candle-free. The feature downloads no model and only
supports the bounded CPU `NativeLinearV1` format.

## 1. Derive identity from bytes

The artifact has 256 deterministic features, two classes, weights, and biases.
It is data only and contains no code or custom operation.

```rust
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
let identity = artifact.identity(None, false)?;
```

`identity` fixes the SHA-256 digest and exact size. Changing one byte makes
`store` or `load` return `AiError::Integrity`. In production, use a `publisher`
and `signature_required = true` with `ProvenanceArtifactStore` when policy
requires signatures.

## 2. Store bytes and describe the model

```rust
let memory = Arc::new(MemoryArtifactStore::new(4 * 1024 * 1024)?);
memory.store(&identity, &bytes, &CancellationToken::new())?;
let store: Arc<dyn ArtifactStore> = memory;

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
    artifact: identity,
};
```

Descriptor ceilings are part of admission. Do not declare values below the
backend's real peak usage.

## 3. Register without implicit discovery

```rust
let backends = Arc::new(BackendRegistry::new());
backends.register(Arc::new(CandleBackend::new(
    store,
    CandleBackendConfig::default(),
)?))?;

let models = Arc::new(ModelRegistry::new());
models.register(descriptor, [ArtifactLocation::Memory])?;
```

The initial state is `Available`: bytes exist, but the backend has not loaded
tensors. The first `resolve` transitions `Available -> Loading -> Ready`.
Duplicate registration fails; there is no silent replacement.

## 4. Admission with explicit capacity

`SystemHardwareProbe::default()` reads real CPU/RAM capacity on macOS, Linux
and Windows and keeps unavailable accelerator metrics as `None`. The example
uses `Custom` to impose a smaller application ceiling than the detected host:

```rust
request.options.resources = AiResourceMode::Custom(AiResourceLimits {
    max_cpu_percent: 80,
    max_memory_bytes: 16 * 1024 * 1024,
    max_vram_bytes: 0,
    max_workers: 1,
    max_concurrent_jobs: 1,
});
```

The backend estimates 75% CPU, one worker, zero VRAM, and RAM equal to the
greater of descriptor memory and twice the artifact size. A lower ceiling
denies the route with `AiError::Capacity("all model routes were denied")`.

## 5. Force model, privacy, and diagnostics

```rust
let mut request = AiRequest::text(AiTask::ClassifyText, "a", limits)?;
request.options.execution = AiExecutionMode::Local;
request.options.privacy = AiPrivacyMode::LocalOnly;
request.options.model = Some(ModelId::new("example/candle-runtime")?);
request.options.resources = custom_limits;
request.options.include_diagnostics = true;

let response = runtime.resolve(request).await?;
```

Forcing the ID prevents another compatible model from being selected.
`LocalOnly` excludes both remote compute and remote storage.
`include_diagnostics` exposes bounded backend/device attempts without copying
input, output, or credentials.

## State and telemetry after the call

```rust
assert_eq!(models.get(&model_id)?.state, ModelState::Ready);
let metrics = runtime.telemetry();
assert_eq!(metrics.requests, 1);
assert_eq!(metrics.model_load_successes, 1);
assert_eq!(metrics.local_placements, 1);
assert_eq!(metrics.successes, 1);
```

A second call reuses the `Ready` model, so `model_load_successes` remains 1.
Percentiles use approximate fixed buckets, and metrics never label model,
tenant, peer, or prompt IDs.

## Explicit failure outcomes

| Situation | Result |
|---|---|
| missing `backend-candle` feature | backend is absent from the compiled API |
| mismatched digest or size | `AiError::Integrity` |
| model has no location | no compatible local route |
| incompatible format/task/device | route excluded before inference |
| insufficient RAM or CPU | candidate denied by admission |
| token cancelled before load | `AiError::Cancelled` |
| elapsed deadline | `AiError::DeadlineExceeded` |
| duplicate backend | `AiError::Conflict("backend id")` |

For checkpoints, resume, and registering a trained descriptor, follow the
[local training recipe](../recipes.en.md#local-reproducible-candle-training).
