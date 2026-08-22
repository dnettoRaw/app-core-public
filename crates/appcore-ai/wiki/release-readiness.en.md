# Beta release: 0.1.0-beta.1

[Português](release-readiness.pt.md) | [Français](release-readiness.fr.md) |
[Performance](benchmarks.en.md) | [Threat model](threat-model.en.md) |
[Generative LLMs](generative-llm.en.md)

Decision on 2026-08-22: release `0.1.0-beta.1` within the support boundary
below. Publication must originate from its clean release commit, and its
immutable tag is created only after the registry package is verified.

The beta claim covers the bounded local orchestration core, resource governor,
cost scheduler, admission, batching, residency, verified artifacts, lightweight
resolver and the explicitly enabled Candle and OpenAI-compatible adapters. It
does not certify every engine or accelerator supported by those adapters.
`swarm` and `appcore-bin/ai-alpha` remain experimental integration surfaces.

## Evidence produced

- the deterministic `perf_lab` measures resolution, registry/scheduler scaling,
  batching, residency, artifacts, Candle/training and Swarm and emits JSONL;
- warm resource snapshot reads reached 167 ns p50 on the reference host, while
  forced dynamic sampling reached 2.416 us p50 and static discovery 2.833 us;
- request revalidation no longer clones payloads; routing avoids repeated scans
  and quadratic recovery, while immutable model metadata is shared;
- hardware sampling and model loading are single-flight; queues reject
  cancelled or expired work and batches honor latency, memory and backend caps;
- native macOS CPU/RAM and Apple unified-memory discovery executed on the
  reference host; Linux/Windows probes and optional NVIDIA NVML cross-compile;
- artifact reads use no-follow opens and handle revalidation, while atomic
  activation and a 32-writer race leave one verified file;
- Candle batches are vectorized and capped at 64, with per-item outcomes;
- Swarm advertisements reject stale replay, duplicate claims and inconsistent
  or oversized metadata; all peer, transfer and learned-route state is bounded;
- the certification soak processed 100,000 exact requests without stuck queue
  or load state; all three fuzz targets compile;
- `default = []` remains; NVIDIA, Candle, HTTP transport, training and Swarm are
  opt-in features.

The [performance report](benchmarks.en.md) records the full before/after table,
including small-batch regressions and the deliberate cost of secure artifact
range reads. The [threat model](threat-model.en.md) records residual risk.

## Beta gate matrix

| Requirement | Beta status | Evidence or boundary |
|---|---|---|
| default-light API and measured `resolve` | PASS | no default ML/HTTP dependency; deterministic benchmark |
| governor, scheduler, queues and batches bounded | PASS | policy tables, contention, cancellation, deadlines and single-flight tests |
| resource-driven placement | PASS | exact-device fit, unified-memory accounting, hysteresis and mode budgets |
| CPU/RAM and Apple unified GPU discovery | PASS on reference macOS arm64 | real `hardware_report` output |
| Linux/Windows probes | IMPLEMENTED, NOT PHYSICALLY CERTIFIED | target cross-compilation; beta testers must validate on hardware |
| NVIDIA/AMD/NPU | PARTIAL | NVML and Linux DRM implemented; NPU remains unavailable rather than simulated |
| artifact integrity and writer races | PASS on reference Unix host | no-follow, handle revalidation and 32-writer race |
| model-load recovery and stress | PASS | 100 concurrent load requests and 100,000-request soak |
| optional Candle/training/OpenAI adapter | PASS locally | feature tests, bounded decoding, 1/8/32 batching and max-64 rejection |
| public API, dependency and feature review | PASS | classified exports, isolated feature graph, package metadata |
| security and supply chain | PASS WITH ACCEPTED WARNING | no known vulnerability; optional Candle graph includes unmaintained `paste` through `gemm` |
| Swarm | EXPERIMENTAL | local planner/validation passes; no production Peer RPC adapter is claimed |
| external engine isolation | DEPLOYMENT-OWNED | Candle is in-process; external engine process/sandbox policy is not owned here |
| declarative V1 composition | OUT OF SCOPE | V1 is frozen; explicit Rust composition remains the supported beta path |

## Deliberate beta limitations

Token streaming, a built-in PDF/OCR engine, automatic model downloads, engine
process management, NPU probing, resumable peer artifact streaming and a
production Swarm transport are not implemented and are not claimed. Unknown
hardware values remain unknown. Cross-platform accelerator certification and a
sustained real-model soak are beta-program evidence, not fabricated local
passes.

Result: **READY FOR BETA** within the scope above. The release procedure is a
clean commit, non-publishing registry preflight, confirmed upload, registry
package verification and only then creation of the immutable tag.
