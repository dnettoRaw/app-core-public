# Storage capability V1 preflight evidence — 2026-08-26

[Português](storage-capability-v1-2026-08-26.pt.md) |
[Français](storage-capability-v1-2026-08-26.fr.md) |
[Guide](../guide.en.md)

Clean-source release-profile certification passed on macOS/aarch64 with Rust
1.97.1 at source commit `12cbfc32264a57eb19b7e5c9e36ce076b3a1aee6`.

| Observation | Result | Gate |
|---|---:|---:|
| Descriptor capability kinds | 7 | exact |
| Provider catalog capacity | 32 | exact |
| Preflight iterations | 16,384 | exact |
| p50 | 42 ns | recorded |
| p95 | 42 ns | recorded |
| p99 | 83 ns | ≤ 1,000,000 ns |
| Throughput | 10,493,879 ops/s | ≥ 10,000 ops/s |
| Unsupported requirement | failed closed | required |
| Whole-suite peak RSS | 320,464 KiB | ≤ 786,432 KiB |

There is no before latency value because the previous host did not perform
storage capability preflight. Its baseline behavior was absence of validation,
not a faster equivalent operation.

```bash
cargo run --release -p appcore-certification -- \
  bottlenecks builds/certification/ac016-bottlenecks.json
```

This evidence certifies the post-1.0 development contract. It does not change
or republish the frozen V1 manifests.
