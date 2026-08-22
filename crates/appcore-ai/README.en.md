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
SemVer. The current version is `0.1.0-beta.1`; it does not change any frozen
AppCore V1 manifest or wire contract.

The default build provides validated requests and responses, explicit
modalities, quality profiles, a deterministic lightweight path,
hardware/resource governance, cost scheduling, bounded fair queues and
batching, per-model/backend single-flight loads, model/artifact registries,
tiered residency, provenance boundaries, redacted telemetry and an asynchronous
`AiRuntime::resolve` API.
It has no ML framework dependency.

The beta release also provides backend-aware adaptive batching, vectorized
Candle batches, bounded LRU model-load coordination and a public
`ModelLoadSnapshot`. Local artifacts use no-follow file opens, handle
revalidation and atomic create-without-replacement activation. Registries,
learned routes, residency, load coordination and Swarm claims have fixed caps.

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

The generative contract includes role-aware chat, bounded sampling, tool
definitions/calls and image inputs. The HTTP adapter executes text/chat and,
when explicitly declared by the server/model, image analysis. PDF is routed as
a first-class document modality but still requires an application-selected
document backend; the core does not embed an unsafe universal PDF/OCR parser.
`SegmentedModelReader` performs verified range reads for AppCore-owned bundles,
without claiming that every engine supports expert streaming.

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

The deliberately experimental `appcore-bin/ai-alpha` feature provides an
explicit Supervisor and `CapabilityRegistry` consumer flow without changing V1
manifests. Declarative selection remains post-1.0 work and is not part of the
beta claim. See the
[release-readiness report](wiki/release-readiness.en.md) and
[threat model](wiki/threat-model.en.md).
