# appcore-ai

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md) |
[Basic example](wiki/examples/basic.en.md) |
[Candle example](wiki/examples/intermediate.en.md) |
[Recipes](wiki/recipes.en.md) |
[Models](wiki/models.en.md) |
[Generative LLMs](wiki/generative-llm.en.md) |
[Hardware resources](wiki/resources.en.md) |
[Performance](wiki/benchmarks.en.md)

Bounded, backend-neutral AI orchestration for AppCore Runtime with independent
SemVer. The current release is `0.1.0-beta.4`; it does not change any stable
AppCore V1 manifest or wire contract.

The default build provides validated requests and responses, explicit
modalities, quality profiles, a deterministic lightweight path,
hardware/resource governance, cost scheduling, bounded fair queues and
batching, per-model/backend single-flight loads, model/artifact registries,
tiered residency, provenance boundaries, redacted telemetry and an asynchronous
`AiRuntime::resolve` API.
It has no ML framework dependency.
Lightweight Unicode whitespace normalization builds only its bounded output
string; it does not retain an intermediate list of every input word.

The beta release also provides backend-aware adaptive batching, vectorized
Candle batches, bounded LRU model-load coordination and a public
`ModelLoadSnapshot`. Local artifacts use no-follow file opens, handle
revalidation and atomic create-without-replacement activation. Registries,
learned routes, residency, load coordination and Swarm claims have fixed caps.
An idempotent local activation or concurrent writer race revalidates and
compares the existing artifact incrementally with one fixed 16 KiB buffer; it
never loads a second complete artifact beside the caller's bytes.
`ArtifactStore::load_lease` lets backends borrow verified bytes for the duration
of decode: memory tiers share their resident `Arc<[u8]>`, while file and peer
tiers keep their existing owned allocation. Active memory leases remain inside
the store byte accounting and prevent eviction until released.
`ModelRegistry::get_lease` and `candidate_leases` likewise return immutable
shared model snapshots. The runtime router uses those leases directly, so
route discovery no longer clones complete descriptors and then clones them
again into local routes. Existing `get` and `candidates` remain owned
compatibility adapters; mutations use copy-on-write and never keep a registry
lock across backend work.
`ModelRegistryLimits` also bounds models, locations per model, total locations
and accounted location bytes. The default ceilings are 4,096 models, 128
locations per model, 65,536 locations and 8 MiB of location metadata; callers
may select tighter limits. Initial iterators and later additions fail before
retention, duplicate additions remain idempotent without copy-on-write, and
`ModelRegistry::pressure` exposes current/peak count and bytes plus rejections.
The Candle loader transfers decoded labels, weights and biases into the loaded
model instead of cloning those complete buffers. `CandleBackend` reserves a
model slot and its declared artifact bytes before store read or decode;
`new_with_loaded_byte_limit` can lower the aggregate byte ceiling and
`memory_pressure` exposes current/peak usage and early rejections. The
reservation follows active inference leases after `unload`. This backend is a
classifier and owns no generative KV cache; external generative engines must
enforce their own cache limit.
OpenAI-compatible encoded bodies use the same immutable shared allocation in
the transport request, blocking-worker handoff and low-level `HttpRequest`.
This removes two possible full-payload copies while retaining the existing
borrowed transport trait and bounded cancellation behavior.

Optional features are explicit:

- `accelerator-nvidia`: read-only NVIDIA VRAM/utilization detection through
  dynamically loaded NVML on Linux/Windows; absent from the default graph;
- `backend-candle`: real CPU inference for bounded `NativeLinearV1` models;
- `backend-openai-compatible`: real bounded chat-completions transport for
  llama.cpp, MLX-LM, TabbyAPI, vLLM, SGLang, TensorRT-LLM, OpenVINO, or an
  explicitly tested compatible server;
- `training-candle`: local reproducible SGD, atomic checkpoints and resume;
- `swarm`: experimental authenticated bridge contracts, expiring peer views,
  separate compute/storage contribution and failover.

GPU discovery is not GPU inference. Candle remains CPU-only even with
`accelerator-nvidia`; both adapters reject unregistered device IDs, and the
HTTP adapter does so before encoding or sending. External physical-device
binding belongs to deployment, not the chat protocol. See the
[execution matrix](wiki/resources.en.md#execution-matrix-detection-is-not-inference).

The generative contract includes role-aware chat, bounded sampling, tool
definitions/calls and image inputs. The HTTP adapter executes text/chat and,
when explicitly declared by the server/model, image analysis. PDF is routed as
a first-class document modality but still requires an application-selected
document backend; the core does not embed an unsafe universal PDF/OCR parser.
`SegmentedModelReader` performs verified range reads for AppCore-owned bundles,
without claiming that every engine supports expert streaming.

This release hardens the OpenAI-compatible boundary with typed HTTP status and
bounded `Retry-After`, recoverable raw tool-call arguments, genuinely
asynchronous transport futures, validated provider compatibility profiles,
opt-in JSON Schema output and cancellable streaming with synchronous
backpressure. Streaming is available only when both the deployment capability
and its transport implementation declare support; the bounded default blocking
HTTP client is offloaded from the caller executor and does not pretend to
provide incremental network delivery.
The SSE decoder parses complete coalesced frames from borrowed transport chunks,
retains only an incomplete tail and compacts pending bytes once per chunk.

Swarm never creates a second control plane or authentication system. A host
adapter must use AppCore security, capability and Peer RPC contracts. Remote
compute requires explicit tenant grants, and peer artifact bytes are verified
before activation.

```bash
cargo test -p appcore-ai
cargo test -p appcore-ai --all-targets --all-features
./crates/appcore-ai/scripts/check-feature-matrix.sh
cargo test -p appcore-ai --test stress_soak --all-features
APPCORE_AI_BENCH_FORMAT=jsonl cargo bench -p appcore-ai --bench perf_lab --all-features
```

`Unrestricted` removes voluntary AppCore headroom only. It cannot disable OS,
driver, firmware, thermal or electrical protections and cannot guarantee that
hardware will not throttle.

Runnable examples:

```bash
cargo run -p appcore-ai --example lightweight_runtime
cargo run -p appcore-ai --example hardware_report
cargo run -p appcore-ai --example candle_runtime --features backend-candle
cargo run -p appcore-ai --example openai_compatible --features backend-openai-compatible
cargo run -p appcore-ai --example candle_training --features training-candle
```

Deployment integration can compose the explicit Supervisor and
`CapabilityRegistry` flow without changing V1 manifests. Declarative selection
remains post-1.0 work and is not part of the beta claim. See the
[release-readiness report](wiki/release-readiness.en.md) and
[threat model](wiki/threat-model.en.md).

## Stable documentation

Stable ID: **ACR-022**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-022). This permanent ID
remains valid if the wiki page moves.
