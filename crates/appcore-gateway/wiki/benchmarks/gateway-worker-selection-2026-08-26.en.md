# Gateway worker-selection benchmark — 2026-08-26

Implementation commit: `8e77c99f18dfee6373e7fe9e0c14aeb5fdd81e39`

The clean release-profile `appcore-dev cert bottlenecks` run used Rust 1.97.1
on macOS/aarch64. It registered 64 workers for one tenant capability and
executed 16,384 selections per measured policy.

| Policy | p50 | p95 | p99 | Maximum | Throughput | Budget |
|---|---:|---:|---:|---:|---:|---:|
| Round-robin | 13,333 ns | 14,958 ns | 17,250 ns | 134,459 ns | 73,341/s | p99 <= 1 ms; >= 10,000/s |
| Least-inflight | 13,750 ns | 14,500 ns | 15,666 ns | 79,583 ns | 71,599/s | p99 <= 1 ms; >= 10,000/s |
| Stateless affinity | 28,709 ns | 30,416 ns | 33,666 ns | 180,500 ns | 34,361/s | p99 <= 1 ms; >= 10,000/s |

Each of 64 workers received exactly four requests in the round-robin
distribution check. Health weighting, queue/capacity rejection and stable
stateless affinity invariants passed. The resolver occupied 16 bytes and the
complete cross-subsystem process peaked at 264,560 KiB under its 786,432 KiB
ceiling.

This is repository-local performance evidence, not a production workload or
cross-platform certification. The affinity keys and worker identities are
fixture values and are not retained by telemetry.
