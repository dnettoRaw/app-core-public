# Gateway connection-hash benchmark — 2026-09-03

The `appcore-dev bench --name appcore-gateway` process workload ran on Apple M1
macOS/aarch64 with Rust 1.97.1. Each result uses five calibrated release samples
after one discarded warmup. The worker case binds 64 distinct capabilities of
128 bytes each, the maximum accepted connection shape.

| Case | Before p50 | Final p50 | Before p95 | Final p95 | Change |
|---|---:|---:|---:|---:|---:|
| Client connection hash | 1.600 us | 1.288 us | 1.638 us | 1.326 us | p50 -19.54% |
| Worker connection hash | 115.765 us | 106.769 us | 119.794 us | 108.556 us | p50 -7.77% |

The worker workload RSS delta fell from 0.34 to 0.27 MiB (-22.73%) and retained
delta from 0.33 to 0.27 MiB (-19.05%). Client workload delta fell from 0.19 to
0.09 MiB. The maximum framing path no longer holds its 8.5 KiB binary frame or
a second 17 KiB payload string beside the required final hexadecimal output.

The comparator passed all six Gateway control and changed cases. These are
repository-local measurements, not cross-platform production claims.
