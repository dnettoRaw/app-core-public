# Gateway capability-registry benchmark — 2026-09-03

The release-profile `appcore-dev bench --name appcore-gateway` run used Rust
1.97.1 on macOS/aarch64 with an Apple M1. Each isolated case retained 1,024
workers advertising the maximum 64 shared capability names. It used one warmup,
five measured process samples and automatic iteration calibration toward 20 ms.

| Reverse index | p50 lookup | p95 lookup | Peak RSS | RSS after teardown |
|---|---:|---:|---:|---:|
| Owned name per worker | 55.53 ns | 59.59 ns | 30.20 MiB | 30.12 MiB |
| Shared name owner | 50.22 ns | 53.20 ns | 27.70 MiB | 24.70 MiB |

The shared registry reduced peak process RSS by 8.28%, RSS after teardown by
18.00% and p50 lookup time by 9.56%. The workload RSS delta fell from 24.75 to
22.25 MiB. The implementation keeps a direct capability-to-worker `HashMap` on
the hot path; registration finds and shares the already-owned immutable name.

Tests also prove duplicate advertisements are normalized, registry clones share
the immutable names, deregistration leaves independent indexes, and removing
the final advertiser releases every registry owner. These measurements are
repository-local evidence, not cross-platform production certification.
