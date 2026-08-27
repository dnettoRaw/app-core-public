# Gateway telemetry benchmark — 2026-08-26

Implementation commit: `31c4fbec34d403770bf59dfe76d36732cb9b4450`

The clean release-profile `appcore-dev cert bottlenecks` run used Rust 1.97.1
on macOS/aarch64. It retained 128 capability series, aggregated eight additional
names into one fixed overflow series, executed 4,096 unavailable-worker routes
and constructed 256 snapshots. Residual inflight was zero and the complete
certification report passed.

| Measurement | p50 | p95 | p99 | Maximum | Throughput | Budget |
|---|---:|---:|---:|---:|---:|---:|
| Instrumented route rejection | 1,666 ns | 1,709 ns | 1,792 ns | 10,125 ns | 591,067/s | p99 <= 1 ms |
| 129-series snapshot | 5,417 ns | 5,708 ns | 5,792 ns | 14,084 ns | 181,281/s | p99 <= 5 ms |

This is repository-local performance evidence, not a production traffic or
collector certification. Prometheus/OpenTelemetry adapters, their queues and
their network failures remain deployment-owned.
