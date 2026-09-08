# Incremental checkpoint benchmark — 2026-09-03

The `appcore-dev bench --name appcore-sync` process workload ran on Apple M1
macOS/aarch64 with Rust 1.97.1. Each result uses five calibrated release samples
after one discarded warmup. The fixture contains 32,768 distinct 128-byte peer
IDs and looks up the final peer while validating every V1 record.

| Measurement | Before | Incremental | Change |
|---|---:|---:|---:|
| p50 | 21.566 ms | 17.305 ms | -19.76% |
| p95 | 21.731 ms | 17.418 ms | -19.85% |
| Peak RSS | 21.50 MiB | 5.67 MiB | -73.62% |
| Workload RSS delta | 16.08 MiB | 0.25 MiB | -98.45% |
| Retained RSS delta | 16.08 MiB | 0.25 MiB | -98.45% |

Startup and lookup now scan with a fixed 16 KiB reader and retain no complete
peer map. Mutation still owns one canonical sorted map to preserve the V1
duplicate and ordering behavior, but writes it directly instead of building a
second file-sized string. File, line and record limits fail closed.

These are repository-local measurements, not cross-platform production claims.
