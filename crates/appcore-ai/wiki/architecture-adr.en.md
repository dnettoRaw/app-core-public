# ADR 0001: AppCore AI orchestration architecture

- Status: accepted for `0.1.0-beta.1` implementation
- Date: 2026-08-21
- Scope: `appcore-ai`; no AppCore V1 manifest or wire-contract change

[Generative LLM profile](generative-llm.en.md) |
[Models and training](models.en.md)

## Context

AppCore needs a backend-neutral way to route bounded AI work without turning
the Runtime into a deep-learning framework. The default build must remain
useful without an LLM or a native accelerator library. Local execution, swarm
compute, and artifact placement must be explicit and independently planned.

The research below uses project documentation and papers current on the ADR
date. Reported performance is not treated as an AppCore result; every AppCore
optimization requires its own reproducible benchmark.

## Comparative evidence

| Project | Technique | Benefit | Cost or constraint | AppCore decision | Phase |
|---|---|---|---|---|---|
| [Lumabri](https://github.com/JustVugg/lumabri) | Independent storage and compute donation, demand fetch, sparse local mirror, replica failover | Lets CPU-only/storage-only peers contribute and preserves hot bytes locally | Experimental network/security surface; first access is network-bound | Adopt separate `ArtifactStore`, `ComputeTarget`, expiring advertisements, and verified local cache; no filesystem hooks | experimental beta |
| [llama.cpp](https://github.com/ggml-org/llama.cpp) | GGUF, broad quantization, CPU/GPU hybrid offload, mmap, continuous batching in the server | Strong local portability and useful operation when VRAM is insufficient | C/C++ build, fast-moving server, native crash boundary | Delivered OpenAI-compatible profile; process lifecycle remains deployment-owned | beta adapter |
| [vLLM](https://docs.vllm.ai/) | PagedAttention, continuous batching, prefix/KV cache, distributed serving | High throughput and lower KV fragmentation for concurrent generation | Heavy Python/GPU serving stack; techniques are workload-specific | Adopt compatibility-key batching and bounded cache accounting, not its API/runtime | later optimization |
| [SGLang](https://github.com/sgl-project/sglang) | RadixAttention prefix reuse, continuous batching, chunked prefill, prefill/decode separation | Efficient repeated prefixes and mixed serving workloads | Heavy serving stack and complex accelerator scheduling | Keep prefix-cache and split-stage extension points private until measured demand | later optimization |
| [Burn](https://burn.dev/books/burn/) | Rust-native model/training workflow with pluggable tensor backends and ONNX import | Coherent optional training and portability story | Framework compile/dependency cost; application must define model semantics | Evaluated but not selected; avoid carrying a second framework before a measured need | research only |
| [Candle](https://huggingface.github.io/candle/) | Rust tensor runtime; CPU, CUDA, Metal and WASM; safetensors/ggml loading | Rust integration and local inference portability | Model/tokenizer integration remains model-specific | Selected for the first opt-in CPU backend and trainer; no Candle types in central contracts | beta adapter |
| [ONNX Runtime](https://onnxruntime.ai/docs/reference/high-level-design.html) | Execution Provider capability discovery, graph partitioning, memory arenas | Mature cross-device execution including CPU, GPU and several NPUs | Native distribution and provider compatibility cost; tensor API is not a text API | Adopt capability-first device selection; candidate bounded tensor backend | future backend |
| [TensorRT-LLM](https://nvidia.github.io/TensorRT-LLM/) | In-flight batching, paged KV cache, quantization, multi-GPU/multi-node execution | High NVIDIA throughput and mature serving optimizations | NVIDIA/Linux specialization and large operational footprint | Delivered compatible-server profile; engine optimizations stay outside the core | beta adapter, external engine |

## Decision

`appcore-ai` is a Runtime-layer orchestration crate with independent SemVer.
Its default feature set uses `std` plus only small integrity dependencies that
are justified at their boundary. Accelerator and training frameworks are
isolated behind opt-in features.

The public entry point is an `AiRuntime` with an observable asynchronous
`resolve` operation. Resolution is a bounded pipeline:

```text
validate request
  -> classify and validate input modalities
  -> try deterministic lightweight resolvers
  -> find compatible model candidates
  -> enforce the requested Fast/Balanced/Deep/Maximum quality floor
  -> calculate local and contribution budgets
  -> plan artifact placement
  -> plan compute placement
  -> admit to a bounded queue/batch
  -> execute through a backend
  -> optionally escalate within a fixed attempt limit
  -> return a redacted decision trace when requested
```

The execution mode is fixed from the first alpha:

```rust
pub enum AiExecutionMode {
    Local,
    Swarm,
    Auto,
}
```

`Auto` may compare or combine local and peer resources. It is not a synonym
for silently allowing remote data transfer: privacy and distribution policies
remain authoritative.

Compute placement and artifact placement are separate decisions:

```text
compute:  local CPU/GPU/NPU | authenticated remote target
storage:  VRAM | RAM | local storage | verified peer store
```

Artifact identity is content-derived and independent of every location. A
peer location can disappear without changing the logical model identity.
Bytes fetched from any peer are bounded and verified before activation.

## Ownership

| Owner | Responsibility |
|---|---|
| core contracts | Validated request/response/options, IDs, policies, limits, cancellation, safe diagnostics |
| lightweight resolver | Deterministic bounded text transforms, rules, classification and extraction |
| router | Candidate ordering, fixed escalation budget and policy enforcement |
| resource governor | Probe snapshots, hysteresis, local budget and separately limited contribution budget |
| scheduler | Deterministic admission and cost score across local or remote compute targets |
| model registry | Model metadata, lifecycle, capabilities and artifact identity/location |
| backend SPI | Load, unload, inference, health and optional specialized training contracts |
| batching | Bounded compatible queues, deadlines, cancellation and partial-failure semantics |
| residency | Bounded promotion, prefetch and eviction across supported storage tiers |
| distributed bridge | Expiring authenticated peer views, compute invocation and artifact transfer boundaries |
| composition root | Provider selection, AppCore capability binding, Supervisor lifecycle and deployment policy |

## Alpha decisions

- The core and all deterministic tests run without a GPU, network or model
  download.
- Resource probes degrade to unknown values; unknown capacity is never treated
  as unlimited capacity.
- `Unrestricted` removes voluntary AppCore headroom only. It cannot alter OS,
  driver, firmware, thermal or electrical protection.
- Queues, retries, peers, metadata, inputs, outputs, artifacts and transfers
  have explicit bounds.
- A remote peer is not an inference backend. Backends describe how to execute;
  compute targets describe where; artifact stores describe where bytes live.
- Swarm remains unavailable unless an authenticated bridge is installed.
  Simulated peers validate planning without claiming a production transport.
- Remote results cannot generally be proven correct cryptographically. The
  alpha can authenticate the peer and verify artifacts but reports this result
  integrity limitation explicitly.
- Candle `0.11` is the single selected experimental ML framework. It is
  available only through `backend-candle` and `training-candle`, and the first
  artifact format is the data-only, bounded `NativeLinearV1` classifier.
- `appcore-bin/ai-alpha` supplies explicit Supervisor and capability composition
  without changing V1; declarative selection remains deferred to a versioned
  post-1.0 contract.

## Resource detection decision

Hardware discovery is a small platform boundary behind `HardwareProbe`, not a
new framework or provider abstraction. Static topology is cached separately
from dynamic counters, and the public `HardwareSampler` is on-demand,
single-flight and bounded. Unknown values remain unknown; failures are redacted
into stable categories.

System CPU/RAM probes use bounded native OS interfaces. Apple integrated GPUs
are modeled with unified memory. Linux DRM sysfs supplies best-effort AMD and
NVIDIA fallback data. NVIDIA NVML is isolated behind the optional
`accelerator-nvidia` feature because portable OS interfaces do not expose its
exact framebuffer memory and utilization. The crate uses only read queries;
there is no clock, power or fan control.

Admission is exact-device. Dedicated VRAM is never summed across GPUs, while
unified memory is charged once to the RAM pool. Resource modes calculate
voluntary headroom from current availability and hysteresis limits churn.
Batching, training, residency and Swarm contribution consume the same bounded
view instead of inventing independent capacity.

This requires narrow documented native FFI on macOS and Windows. The crate
therefore uses `#![deny(unsafe_code)]`, with `allow` scoped only to those two
platform modules. All backend-neutral resource and policy code remains safe
Rust. See [hardware resources](resources.en.md).

## Beta generative implementation and remaining boundaries

The beta delivers:

- one bounded OpenAI-compatible server adapter and seven explicit engine profiles;
- role-aware chat, sampling, tools/tool calls, usage and opt-in image data URLs;
- persistent external engine, loopback by default and no inference-time download;
- AppCore segment manifests and verified local range reads;
- per-model/backend single-flight load coordination across fallback and concurrency;
- explicit opt-in `appcore-bin` lifecycle and local capability registration.

Native token streaming requires an explicitly capable deployment transport.
Still outside the claim: PDF/OCR, automatic process launch or sandbox,
engine-owned KV accounting, expert streaming without a consuming
backend, and declarative V2 manifests.

This boundary keeps native crashes, tokenizers, KV cache, and kernels outside
the backend-neutral core. The [generative profile](generative-llm.en.md)
contains models, budgets, commands, and gates.

## Not in `0.1.0`

- a home-grown deep-learning framework or tensor implementation;
- silent model downloads or vendor-specific registries;
- unbounded continuous batching, cache, queue or artifact transfer;
- arbitrary model code/custom-op execution by default;
- distributed training, RAFT, multi-master state, or global leadership;
- NAT traversal, a second control plane, a second authentication system, or a
  parallel peer protocol;
- a claim that `Unrestricted` disables hardware safeguards;
- a stable AppCore V1 manifest extension;
- promotion to RC or stable without the evidence required by the phase gates.

## Consequences

This design adds orchestration boundaries before expensive engines. It costs
more explicit configuration, and enabling Candle adds a materially larger
optional dependency tree. Some modes initially return a typed unavailable
error. In exchange, the default core remains portable and testable, backend
churn stays behind the SPI, and future AppCore integration can use a new
versioned contract without weakening V1.

## 2026-08-25 beta.2 amendment

The OpenAI-compatible SPI now returns boxed futures so a deployment can supply
native asynchronous HTTP without choosing an executor for the core crate. The
bounded default client isolates its blocking standalone transport behind a
maximum number of short-lived worker threads. It never blocks the caller
executor and rejects excess work instead of growing an unbounded queue.

Streaming uses a synchronous `AiStreamSink`: returning from one event grants
permission to read the next chunk. This makes backpressure explicit without a
runtime-specific channel. Cancellation is checked between chunks, partial
output is never presented as a complete response, and raw content remains out
of built-in diagnostics. Provider extensions are bounded JSON values with
reserved core fields; JSON Schema fallback is always caller-selected. No V1
manifest or wire contract changes.
