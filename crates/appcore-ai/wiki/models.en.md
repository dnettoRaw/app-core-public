# Models, configuration, and training

[Português](models.pt.md) | [Français](models.fr.md) |
[Guide](guide.en.md) | [Generative LLMs](generative-llm.en.md) |
[Candle example](examples/intermediate.en.md) | [Recipes](recipes.en.md)

This page separates recognized formats, executable backends, and implemented
training. In `0.1.0-beta.1`, registering format metadata does not mean an engine
can infer that format.

## Actual support matrix

| Format | Default policy | Bundled backend | Bundled training |
|---|---:|---:|---:|
| `NativeLinearV1` | allowed | Candle CPU | local linear classification |
| GGUF | allowed | OpenAI-compatible llama.cpp/generic server | none |
| ONNX | allowed | OpenAI-compatible OpenVINO/generic server | none |
| SafeTensors | allowed | OpenAI-compatible MLX/vLLM/SGLang/TensorRT/Tabby/generic server | none |
| `Other(CapabilityId)` | denied by default | adapter required | adapter required |

The crate bundles `candle/cpu-linear-v1` and the opt-in
`OpenAiCompatibleBackend`. The latter talks to an already running server; it
does not parse or execute GGUF/ONNX/SafeTensors itself and never silently
downloads a model. Exact server model binding, digest, device, format and
capabilities remain mandatory deployment configuration.

`ModelDescriptor::input_modalities` and `BackendDescriptor::input_modalities`
declare the real intersection accepted by a route. The router rejects an image
sent to a text-only adapter even if its model ID was registered incorrectly.
`AiOptions::quality` also filters the minimum `QualityTier` automatically. A
forced model ID cannot bypass modality, format, resource, or privacy
constraints.

## Configure an existing generative model

Enable `backend-openai-compatible`, start the selected engine separately on
loopback, and bind an AppCore `ModelId` to its exact server model name. The
executable example requires the real artifact digest instead of inventing one:

```bash
APPCORE_AI_ENGINE=llama.cpp \
APPCORE_AI_FORMAT=gguf \
APPCORE_AI_BASE_URL=http://127.0.0.1:8080 \
APPCORE_AI_MODEL=my-model \
APPCORE_AI_MODEL_SHA256=<64-hex-digest> \
APPCORE_AI_MODEL_BYTES=<exact-size> \
cargo run -p appcore-ai --example openai_compatible \
  --features backend-openai-compatible
```

The profiles accepted by `APPCORE_AI_ENGINE` are `llama.cpp`, `mlx-lm`,
`vllm`, `sglang`, `tensorrt-llm`, `openvino`, `tabbyapi`, and `generic`.
Capabilities such as tools, vision, seed and stop sequences must be enabled in
`OpenAiCompatibleConfig` only after the exact server/model combination proves
them. The default transport rejects credentials; remote authentication requires
an AppCore security-backed transport implementation.

## Configure an existing NativeLinearV1

Enable inference only:

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.1", default-features = false, features = ["backend-candle"] }
```

Build or import the `[classes, input_dimensions]` matrix, biases, and labels:

```rust
let dimensions = 256;
let labels = vec!["available".into(), "unavailable".into()];
let weights = vec![0.0_f32; labels.len() * dimensions];
let biases = vec![0.0_f32; labels.len()];
let artifact = NativeLinearArtifact::new(
    dimensions,
    labels,
    weights,
    biases,
)?;
let bytes = artifact.encode()?;
let identity = artifact.identity(None, false)?;
```

Then:

1. write bytes to an `ArtifactStore`;
2. create a `ModelDescriptor` with `ArtifactFormat::NativeLinearV1`;
3. register `CandleBackend` in `BackendRegistry`;
4. register the descriptor and location in `ModelRegistry`;
5. submit `AiTask::ClassifyText` through `AiRuntime`.

The complete flow is in
[`candle_runtime.rs`](../examples/candle_runtime.rs). Weights are `f32`; declare
`Quantization::None`. Other `Quantization` values exist for future backends and
do not quantize this format automatically.

## Train local classification

Enable:

```toml
[dependencies]
appcore-ai = { version = "0.1.0-beta.1", default-features = false, features = ["training-candle"] }
```

Each example contains non-empty text and a class index:

```rust
let dataset: Arc<dyn TrainingDataset> = Arc::new(
    InMemoryTrainingDataset::new(
        vec![
            TrainingExample { text: "service ready".into(), label: 0 },
            TrainingExample { text: "healthy".into(), label: 0 },
            TrainingExample { text: "service failed".into(), label: 1 },
            TrainingExample { text: "unavailable".into(), label: 1 },
        ],
        1_000,
        512,
    )?,
);
```

Configure every job bound:

```rust
let job = TrainingJob {
    id: CapabilityId::new("job/service-status")?,
    model: ModelId::new("model/service-status")?,
    revision: "v1".into(),
    labels: vec!["available".into(), "unavailable".into()],
    input_dimensions: 256,
    epochs: 20,
    max_steps: 1_000,
    batch_size: 16,
    learning_rate: 0.1,
    seed: 42,
    resource_requirements: ResourceEstimate {
        cpu_percent: 60,
        memory_bytes: 32 * 1024 * 1024,
        workers: 1,
        ..ResourceEstimate::default()
    },
    resource_mode: AiResourceMode::Custom(AiResourceLimits {
        max_cpu_percent: 70,
        max_memory_bytes: 64 * 1024 * 1024,
        max_vram_bytes: 0,
        max_workers: 1,
        max_concurrent_jobs: 1,
    }),
    checkpoints: TrainingCheckpointPolicy {
        every_epochs: 5,
        max_checkpoints: 4,
    },
    resume: None,
    publisher: None,
    max_input_bytes: 512,
    max_output_bytes: 4 * 1024,
};
```

Execute and register the result:

```rust
let output = trainer
    .train(&job, dataset, progress, &cancellation)
    .await?;
models.register(output.descriptor.clone(), [ArtifactLocation::Memory])?;
```

Use the same `ArtifactStore` for trainer and backend: `CandleTrainer` already
writes final artifacts and checkpoints. The reproducible program is
[`candle_training.rs`](../examples/candle_training.rs):

```bash
cargo run -p appcore-ai --example candle_training --features training-candle
```

## Parameter meaning

| Field | Effect |
|---|---|
| `labels` | stable class order; dataset indices refer to this order |
| `input_dimensions` | hashed feature-vector width, not token count |
| `epochs` | maximum complete dataset passes |
| `max_steps` | global ceiling that may stop before the final epoch |
| `batch_size` | job request; conservative modes may reduce it |
| `learning_rate` | finite positive SGD rate |
| `seed` | reproducible weight initialization |
| `resource_requirements` | declared peak before admission |
| `checkpoints` | maximum snapshot frequency and count |
| `resume` | exact identity of a compatible `NativeLinearV1` |

`Eco` uses effective batch size 1. `Balanced` and `Custom` halve the requested
batch, rounding up. `Performance` and `Unrestricted` preserve the request,
still within trainer ceilings.

## Effective default limits

| Candle trainer limit | Default |
|---|---:|
| examples | 100,000 |
| dimensions | 4,096 |
| classes | 256, with a minimum of 2 for training |
| epochs | 100 |
| optimizer steps | 100,000 |
| batch | 512 |
| encoded artifact | 64 MiB |

Labels are at most 96 bytes. Each text must satisfy both dataset limits and
`job.max_input_bytes`. The effective ceiling is always the minimum across
policy, job, backend, artifact store, and request.

## Choose dimensions and data

`NativeLinearV1` uses deterministic features derived from text bytes. It fits
simple lexical signals, routing, filters, and small classification. As a
starting point, not a quality guarantee:

| Problem | Initial dimensions |
|---|---:|
| up to 20 labels, small vocabulary | 256–512 |
| 20–100 labels or larger vocabulary | 1,024–2,048 |
| up to 256 labels | 2,048–4,096, measuring collisions and RAM |

Split training and validation, keep examples balanced, and measure precision,
recall, and a confusion matrix. More epochs cannot fix bad labels, ambiguous
classes, or unrepresentative data.

## Resume, identity, and provenance

Resume requires identical dimensions and labels in identical order. In this
beta, the trainer accepts a local-only identity with `signature_required = false`;
signed-resume integration is not yet connected. Never change bytes while
retaining an ID: SHA-256 and exact size are the actual identity.

For signed inference activation, use `ProvenanceArtifactStore` with a verifier
from AppCore security. `ModelSecurityPolicy::default()` allows NativeLinearV1,
GGUF, ONNX, and SafeTensors, denies provider formats, and caps artifact/RAM/VRAM.
A deployment should lower those maxima to real hardware.

## This is not LLM training

The current trainer does no pretraining, fine-tuning, LoRA, generation,
embeddings, image, audio, GPU, or distributed training. Train or fine-tune LLMs
outside the Runtime, convert them to a data-only format, and activate them
through an explicit backend. See the [generative LLM profile](generative-llm.en.md).
