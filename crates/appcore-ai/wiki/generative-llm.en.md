# Adaptive runtime for LLMs and multimodal AI

[Português](generative-llm.pt.md) | [Français](generative-llm.fr.md) |
[Guide](guide.en.md) | [Models and training](models.en.md) |
[Architecture](architecture-adr.en.md) | [Threat model](threat-model.en.md)

> Status: `backend-openai-compatible` delivers bounded text/chat/tool and
> opt-in vision transport for explicitly configured servers. Candle remains
> the data-only classifier backend. `AnalyzeDocument` still needs a document
> backend; no universal PDF/OCR parser is embedded.

The target is not one universal engine and not a mandatory dependency on
another runtime. `appcore-ai` is the control plane that selects the best
installed route for the request, hardware, and policy. Every engine remains
isolated behind `InferenceBackend` and can be replaced without changing the
application.

Colibri is not a dependency or an engine profile. AppCore owns generic,
verifiable VRAM/RAM/local-storage planning plus a segment manifest and verified
range reader. Expert streaming is claimed only by a backend that actually
consumes those segments.

Recommendations were reviewed on 2026-08-21. Pin engine version, artifact
digest, format, tokenizer, chat template, and license per deployment.

## What “Swiss army knife” means

```text
AiRequest
  task       -> generate text | analyze image | analyze document | decide | embed
  input      -> Text | Image | Document | Audio | Video | Opaque
  quality    -> Fast | Balanced | Deep | Maximum
  latency    -> Interactive | Balanced | Throughput | Background
  placement  -> Local | Swarm | Auto
        |
        v
validation and privacy
  -> deterministic path when sufficient
  -> models and adapters compatible with every input modality
  -> RAM/VRAM/CPU/deadline admission
  -> load, queue, latency, throughput, residency, and cost scoring
  -> selected persistent engine
  -> bounded response and redacted diagnostics
```

Supporting many engines means accepting conforming adapters, not compiling all
frameworks into the core. The default build remains small, and a deployment
enables only the adapters it needs.

## Speed versus depth

`AiOptions::quality` is an enforced floor:

| Profile | Minimum `QualityTier` | Typical use |
|---|---|---|
| `Fast` | `Tiny` | UI, autocomplete, classification, short answers |
| `Balanced` | `Small` | general local assistant |
| `Deep` | `Balanced` | documents, code, and harder analysis |
| `Maximum` | `Large` | quality ahead of latency and resource cost |

```rust
let mut request = AiRequest::text(
    AiTask::GenerateText,
    "Compare the alternatives and justify the conclusion.",
    AiLimits::default(),
)?;
request.options.quality = AiQualityTarget::Deep;
request.options.latency = AiLatencyClass::Balanced;
request.options.execution = AiExecutionMode::Auto;
request.options.allow_escalation = true;
request.options.deadline = Some(Duration::from_secs(45));

let answer = ai.resolve(request).await?;
```

An explicitly forced model bypasses the automatic quality filter, but must
still satisfy format, modalities, device, privacy, resources, and deadline.
There is no silent model or quantization downgrade.

`Deep` does not authorize unbounded autonomous loops or chain-of-thought
exposure. A future multi-stage planner may run bounded draft, verification, and
synthesis stages. Applications continue to own prompts, tools, schemas, and
business policy.

## Images and PDFs

The beta validates first-class modalities:

```rust
let input = AiInput::new(
    vec![
        AiContent::Text("List the risks visible in this image".into()),
        AiContent::Binary {
            media_type: "image/png".into(),
            bytes: image_bytes,
        },
    ],
    limits,
)?;
let request = AiRequest {
    task: AiTask::AnalyzeImage,
    input,
    options: AiOptions::default(),
};
```

For PDF:

```rust
let input = AiInput::new(
    vec![AiContent::Binary {
        media_type: "application/pdf".into(),
        bytes: pdf_bytes,
    }],
    limits,
)?;
let request = AiRequest {
    task: AiTask::AnalyzeDocument,
    input,
    options: AiOptions {
        quality: AiQualityTarget::Deep,
        ..AiOptions::default()
    },
};
```

A PDF is a container, not a native modality of every VLM. An adapter may use
native document support, extract bounded text with page references, rasterize
admitted pages, or run bounded OCR. A document processor must cap page count,
pixels, expanded bytes, duration, and output. The core does not embed a
PDF parser, OCR stack, or image decoder, and document processing must never
fetch external resources implicitly.

## Decision systems

`AiTask::Decide` does not make a completion authoritative:

```text
deterministic rule
  -> small classifier when the rule is insufficient
  -> LLM/VLM only for policy-permitted ambiguity
  -> schema and confidence validation
  -> application policy accepts, rejects, or requests review
```

Rules, outcomes, and thresholds are application-owned. The Runtime provides
bounds, routing, redacted audit, and route diagnostics. Privileged actions must
use structured output, referenced evidence, and a safe fallback rather than raw
generated text.

## Supported server profiles

The `backend-openai-compatible` feature has explicit profiles for every server
family below. The engine remains a separately deployed process; the feature
does not install, download, start, or sandbox it. A profile supplies safe format
defaults, while each deployment explicitly declares model names, devices,
vision/tools/seed/stop support, endpoint policy, timeout, and byte limits.

| Hardware/workload | Recommended adapter | Reason | Trade-off |
|---|---|---|---|
| CPU, GGUF, mixed hardware | [llama.cpp](https://github.com/ggml-org/llama.cpp) | broad coverage, quantization, hybrid CPU/GPU | not always fastest per device |
| Apple Silicon | [MLX-LM](https://github.com/ml-explore/mlx-lm) plus an MLX VLM driver | unified memory and native kernels | Apple-specific |
| Consumer NVIDIA, low concurrency | [ExLlamaV3](https://github.com/turboderp-org/exllamav3) through TabbyAPI | EXL3 and consumer-GPU focus | specialized format/ecosystem |
| NVIDIA/AMD, high concurrency | [SGLang](https://www.sglang.io/) or [vLLM](https://docs.vllm.ai/en/stable/) | continuous batching, KV cache, multimodal serving | larger operational stack |
| NVIDIA, peak performance | [TensorRT-LLM](https://docs.nvidia.com/tensorrt-llm/) | NVIDIA kernels, quantization, and serving | hardware coupling |
| Intel CPU/GPU/NPU | [OpenVINO GenAI](https://docs.openvino.ai/2026/openvino-workflow-generative/inference-with-genai.html) | optimized LLM/VLM pipelines | dedicated conversion and formats |
| small Rust classifier | current Candle backend | in-process, data-only, auditable | not a generative LLM |

Selection benchmarks the complete tuple:

```text
engine version + model revision + quantization + context + batch + device
```

At minimum it records cold start, TTFT, prompt tokens/s, decode tokens/s,
requests/s, RAM, VRAM, queue depth, and error rate. A configuration wins only
for the workload class in which it was measured.

## Delivered common server adapter

`OpenAiCompatibleBackend` translates the central contract once for the listed
servers. It provides:

- explicit loopback-by-default endpoint policy;
- exact AppCore model ID to server model-name bindings;
- role-aware messages, bounded sampling, tools/tool calls and image data URLs;
- declared capability checks before transport;
- bounded request/response bytes, timeout and cancellation checks;
- bounded fair admission around model routes;
- stable errors without provider body or private process output;
- a transport trait that receives only an unresolved AppCore secret reference.

The default HTTP transport rejects credential references and therefore fits
unauthenticated loopback/private endpoints only. A remote deployment supplies a
transport backed by AppCore security and uses the explicit
`OpenAiCompatibleConfig::remote` constructor. The engine process stays loaded
so model, prefix and KV caches survive requests; process launch, health probes
and OS sandboxing remain deployment responsibilities.

```rust
let config = OpenAiCompatibleConfig::local(
    OpenAiCompatibleEngine::LlamaCpp,
    BackendId::new("local/llama")?,
    "http://127.0.0.1:8080",
    vec![BackendDevice {
        id: DeviceId::new("local/gpu")?,
        kind: DeviceKind::Gpu,
    }],
    model_names,
)?;
let backend = OpenAiCompatibleBackend::new(
    config,
    Arc::new(UnauthenticatedOpenAiHttpTransport),
)?;
```

Run the complete example with an already running compatible server:

```bash
APPCORE_AI_BASE_URL=http://127.0.0.1:8080 \
APPCORE_AI_MODEL=my-model \
APPCORE_AI_MODEL_SHA256=<64-hex-digest> \
cargo run -p appcore-ai --example openai_compatible \
  --features backend-openai-compatible
```

## AppCore-owned residency

Whole-model residency still uses:

```text
ArtifactIdentity -> Vram(device) | Memory | LocalStorage | Peer(peer)
```

`ArtifactBundleManifest` and `SegmentedModelReader` now implement the
engine-independent range boundary:

```text
ModelBundle
  dense/tokenizer/config  -> preferably resident
  segment 000..N          -> digest + size + offset + class
  access observations     -> bounded hot/warm/cold data
  placement               -> VRAM -> RAM -> mmap/NVMe -> verified peer
```

The reader validates sorted non-overlapping ranges, request/segment limits and
SHA-256 for every loaded segment, then uses `LocalArtifactCache::load_range`
without allocating the complete artifact. The core plans bytes and tiers while
adapters own tensors and kernels. Prefetch, cache, eviction, rollback,
and I/O pressure remain bounded and observable. No peer may force local
residency, and no `LD_PRELOAD`, filesystem hook, or hidden third-party format
is used. Until a real backend consumes this bundle, the project does not claim
expert streaming and the governor rejects models that do not fit.

## Initial model families

| Model | Modalities/use | Initial profile |
|---|---|---|
| [Qwen3 4B](https://huggingface.co/Qwen/Qwen3-4B) | local multilingual text | `Fast`/`Balanced`, GGUF Q4/Q5 |
| [Qwen3 8B](https://huggingface.co/Qwen/Qwen3-8B) | stronger general answers | `Balanced`/`Deep` |
| [Gemma 3 12B IT](https://huggingface.co/google/gemma-3-12b-it) | text and images | `Deep` VLM; accept Gemma terms |
| [Phi-4 multimodal](https://huggingface.co/microsoft/Phi-4-multimodal-instruct) | text, image, and audio | SafeTensors/OpenVINO after conformance |
| [Mistral Small 3.2 24B](https://huggingface.co/mistralai/Mistral-Small-3.2-24B-Instruct-2506) | text, vision, tools, documents | `Deep`/`Maximum`, larger hardware |

Never enable `trust_remote_code`. A custom-code model requires an explicitly
reviewed adapter and policy. See [models and training](models.en.md) for the
support actually delivered.

## Delivered scope and external boundaries

Delivered in beta: modality/quality routing, role-aware chat, bounded sampling
and tools, the common server adapter, seven engine profiles, a real loopback
conformance test, fair execution admission, segment manifests/range reads, and
opt-in `appcore-bin` Supervisor/capability composition.

Not claimed: token streaming, engine-owned KV-cache accounting, automatic
engine installation/process sandboxing, PDF/OCR, expert streaming, a production
Peer RPC Swarm adapter, or a V2 declarative manifest. Those remain explicit
boundaries rather than documentation promises. The core does not promise any model
on any machine; it explains why a route was admitted or rejected.
