# Performance laboratory and optimization report

[Português](benchmarks.pt.md) | [Français](benchmarks.fr.md) | [Guide](guide.en.md)

This report compares the same deterministic `perf_lab` workloads before and
after the 2026-08-22 alpha hardening pass. Baseline is the initial run; final
latencies are the median of five release-process runs. The resource section
states its separate baseline and measurement protocol. It is engineering
evidence, not a portable guarantee or a CI threshold.

## Reproduce the measurements

```bash
cargo bench -p appcore-ai --bench perf_lab --all-features
APPCORE_AI_BENCH_FORMAT=jsonl \
  cargo bench -p appcore-ai --bench perf_lab --all-features
cargo bench -p appcore-ai --bench alpha_harness --all-features
```

JSON Lines output contains the workload name, iterations, throughput, wall
time and p50/p95/p99 nanoseconds. The harness uses fixed data and bounds, but it
does not pin CPU frequency or suppress other host work. Compare distributions
and rerun on deployment hardware instead of treating one number as an SLO.
`artifact_memory_owned_1mib` and `artifact_memory_lease_1mib` are paired
controls over the same verified resident bytes: the first requests unique
`Vec<u8>` ownership, while the second exercises the shared lease used by Candle.

Reference host: Apple M1 MacBookPro17,1, 16 GiB RAM, Darwin arm64,
`rustc 1.97.1`, release build. The final process was executed directly after
Cargo built it so compiler memory was excluded. With the explicit 1 MiB request
validation workload, macOS reported 11.4 MiB maximum resident set and 6.5 MiB
peak footprint.

## Before and after

| Workload | Baseline p50 | Final p50 | Change |
|---|---:|---:|---:|
| lightweight resolve hit | 583 ns | 500 ns | -14.2% |
| missing model route | 583 ns | 542 ns | -7.0% |
| warm backend, 1 route | 2.250 us | 2.042 us | -9.2% |
| warm backend, 32 routes | 96.417 us | 21.958 us | **-77.2%** |
| cold unique model load | 2.875 us | 2.625 us | -8.7% |
| scheduler, 32 candidates | 4.834 us | 4.500 us | -6.9% |
| local artifact, full 1 MiB | 3.409 ms | 3.086 ms | -9.5% |
| local artifact, 4 KiB range | 16.583 us | 24.667 us | +48.7% |
| Candle batch, 1 item | 2.250 us | 2.375 us | +5.6% |
| Candle batch, 8 items | 17.708 us | 18.708 us | +5.6% |
| Candle batch, 32 items | 68.959 us | 31.041 us | **-55.0%** |
| Swarm scheduler, 1,000 peers | 226.958 us | 218.625 us | -3.7% |

The small Candle batch-1/batch-8 differences are sub-microsecond absolute costs
plus run-to-run noise; they are reported rather than hidden. Batch 32 shows the
vectorized path's intended crossover. The range-read regression is an
intentional security cost: every local read now uses
no-follow open semantics and validates the opened handle, regular-file type
and exact size to close symlink/reparse substitution races.

Additional final p50 scaling points:

| Component | 1 | 32 | 128 |
|---|---:|---:|---:|
| owned model registry candidates | 333 ns | 8.375 us | 28.667 us |
| shared model registry leases | 42 ns | 375 ns | 875 ns |
| cost scheduler | 125 ns | 4.500 us | 20.583 us |

Swarm is measured directly at 1/10/100/1,000 peers; exact values are in JSONL.
Final batching p50 was 458 ns, 625 ns, 875 ns, 1.417 us and 2.542 us for
1/2/4/8/16 items. Tiny Candle training over 64 examples and two epochs measured
311.750 us p50. Borrowed validation of a 1 MiB binary request measured 42 ns
p50 versus 19.958 us for an explicit clone control, about 475 times apart. The
control demonstrates the removed copy; it is not presented as a second
historical baseline run.

The paired 2026-09-02 registry run used 2,000 iterations per size. At 128
models the shared lease path was about 33 times faster than the compatible
owned adapter and retained only `Arc` handles for the result. The production
router uses the lease path and transfers the same allocations into local
routes.

The 2026-09-02 release verification run of the new paired artifact controls
measured 25.333 us p50 for unique owned loading and 42 ns for the shared lease,
about 603 times apart on the reference host. This single run verifies the
workload and expected ownership cost; rerun distributions before using it for
a release comparison.

The calibrated 2026-09-03 `appcore-dev bench --name appcore-ai` comparison
measured idempotent revalidation of an existing 32 MiB local artifact. Five
process samples changed p50 from 215.29 ms to 215.83 ms (+0.25%, inside the
noise allowance) and reduced peak RSS from 65.86 MiB to 33.84 MiB (-48.61%);
workload and retained RSS deltas fell from 32.03 MiB to zero because validation
now uses a fixed 16 KiB buffer.

The same lab now includes `openai_sse_coalesced_256x4kib`, which supplies 256
complete 4 KiB text frames in one bounded transport chunk. Across five release
processes, median p50 fell from 46.78 ms to 44.27 ms (-5.36%), median p95 from
47.35 ms to 45.09 ms (-4.76%) and median throughput rose about 5.3%. The
production path parses those frames from the borrowed chunk with zero pending
capacity; fragmented input retains only the incomplete tail.

The `model_registry_location_pressure_65536` process workload exercises 65,536
distinct peer locations. Before bounded admission it retained every location,
with 35.995 ms p50 and 7.83 MiB peak/retained RSS. With the default per-model
ceiling, it retains 128, rejects the remaining additions before copy-on-write
and reports the pressure counters. Five processes measured 25.218 ms p50
(-29.94%), 1.91 MiB peak RSS (-75.61%) and 1.89 MiB retained RSS (-75.86%).
This is a capacity-boundary workload: its purpose is to prove bounded
retention and rejection, not equal accepted work before and after.

The `candle_model_load_4096x256` process workload loads a representative
4 MiB `NativeLinearV1` classifier into a fresh backend. Before the decoded
vectors were moved into Candle, five release processes measured 1.701 ms
median load time and 20.16 MiB median peak RSS. Moving labels, weights and
biases, then adding pre-read count/byte reservations, measured 1.368 ms
(-19.56%) and 16.11 MiB (-20.08%). The byte gauge accounts the exact encoded
artifact size; map/tensor overhead remains bounded by the model-count ceiling.

The `lightweight_normalize_1mib_words` process workload exercises the default
1 MiB input ceiling with 524,288 one-byte words. The old collect-then-join path
retained one `Vec<&str>` entry per word. Five release processes measured
4.823 ms median time and 12.98 MiB median peak RSS. Writing words directly into
the output `String` measured 3.337 ms (-30.80%) and 3.88 MiB (-70.16%) while
preserving Unicode whitespace semantics.

The production resource path was measured separately on the same Apple M1:

| Default-light `alpha_harness` operation | Before p50 | Final median p50 | Change |
|---|---:|---:|---:|
| lightweight resolve | 1.542 us | 875 ns | -43.3% |
| shared cached snapshot | 541 ns | 167 ns | -69.1% |

| Resource operation | Final p50 | p95 | Meaning |
|---|---:|---:|---|
| shared cached snapshot | 167 ns | 208 ns | normal request/scheduler read |
| forced dynamic sample | 2.416 us | 3.208 us | native CPU/RAM/device refresh; diagnostics only |
| independent static discovery | 2.833 us | 3.834 us | new sampler/platform topology setup |

The final hot-path values are the median of five consecutive release runs; the
before value is the recorded pre-change run. The dynamic/static table is the
separate all-feature `perf_lab` run. The old snapshot did not collect the new
CPU/RAM/device detail, so its improvement demonstrates the cache boundary, not
faster native OS reads. Physical sampling is outside the per-request hot path.
An idle-sampler test waits without reading and observes zero probe calls; the
sampler owns no polling thread or history buffer.

## Hotspots and changes

The initial priority order was route construction, Candle per-item execution,
artifact I/O, scheduler scans, then lock contention. The implementation now:

- precomputes request modalities/bytes, performs direct forced-backend lookup
  and maps scored routes without an O(n squared) recovery scan;
- revalidates borrowed request parts instead of cloning text/image/document
  payloads at the start of every `resolve`;
- shares immutable model records with `Arc` across planned routes;
- performs one vectorized Candle matrix multiply and softmax for batches up to
  64, while retaining per-item validation and errors;
- performs hardware probe I/O outside the governor mutex and collapses
  concurrent refreshes into one sample, including bounded failure caching;
- keeps model loads single-flight with bounded LRU-ready state and exposes
  load/wait/hit/eviction/invalidation counters;
- adapts batching to latency class, pressure, memory and backend limit;
- shares immutable memory-artifact bytes through a short-lived lease; only the
  explicit owned compatibility path copies them;
- bounds registries, learned scheduler entries, residency state, load routes,
  peers, claims and transfers.

No result cache was added to `resolve`: outputs may be sensitive,
backend-dependent or non-deterministic. Reuse is limited to resource samples,
verified immutable artifacts, ready model loads and residency metadata, all
with explicit invalidation or fixed capacity.

## CPU, memory and concurrency evidence

The measured final suite completed in 1.14 s wall, 0.62 s user and 0.10 s system
on the reference host. These are process totals, not per-request CPU budgets.
The stress test performs 20,000 default lightweight requests (up to 1,000,000
with `APPCORE_AI_SOAK_ITERATIONS`) and checks exact telemetry plus empty queue
and load gauges at completion. Certification extended it to 100,000 requests.

Concurrency tests cover 100 requests sharing one cold model load, 32 concurrent
artifact writers, cancellation/deadline removal before dispatch, single-flight
resource sampling, queue saturation and 1,000-peer churn. Fuzz targets cover
native artifacts, contract boundaries and bounded OpenAI response decoding.

Logical memory ownership is bounded independently of RSS:

| Owner | Default or hard bound | Allocation behavior |
|---|---|---|
| request/response | 1 MiB each, 16 input parts, 3 attempts | borrowed revalidation; no deep payload clone in `resolve` |
| execution queue | 8 active + 128 waiting by default | capacity error before unbounded growth |
| dynamic batcher | 32 keys, 256 total, 64/key, 16/dispatch by default | selected backend may lower the dispatch; direct Candle rejects above 64 |
| registries/planners | 4,096 models, 256 backends, 4,096 learned/load/resident routes, 256 pending reservations | fixed-cap maps; ready load state uses LRU eviction |
| artifact | caller-set aggregate memory/store maximum | `load_lease` shares resident bytes and blocks eviction while active; owned `load` copies by contract; range allocates only range bytes |
| Swarm directory | 4,096 peers hard max; 64 devices and 1,024 artifacts/peer | fixed metadata/transfer caps; no model bytes in generic RPC |
| Candle inference/training | 64 inference batch; training defaults 512 batch, 4,096 dimensions, 256 classes, 64 MiB artifact | dataset trait loads one bounded example by index and can be paged/file-backed |

Allocator-call counts were not instrumented because this host has no allocator
profiler in the certification path and the crate does not install an intrusive
global allocator. Peak process memory, request/artifact copy controls and
logical caps are measured or verified; allocation-call profiling remains
external RC/certification evidence.

## Interpretation limits

The HTTP adapter benchmark excludes network and real model execution. Candle
uses a tiny linear classifier, not an LLM. Filesystem cache affects artifact
results. GPU/NPU startup, physical NVIDIA/AMD probes, real GGUF/MLX engines,
tokens/s, energy, thermal throttling and remote tails require deployment-specific
measurement. The hardware report executed on Apple M1; Linux/Windows and
NVIDIA feature evidence is compilation and deterministic tests, not physical
certification.
