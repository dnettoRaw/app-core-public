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

## Borrowed-candidate follow-up — 2026-09-03

The process workload `worker_selection_round_robin_1024` exercises the
tenant-local worker ceiling. Five calibrated release samples with one warmup
reduced p50 from 468.86 to 335.56 us (-28.43%) and p95 from 471.98 to
339.67 us (-28.03%). The candidate metadata slot fell from 104 to 40 bytes on
macOS/aarch64, excluding the eliminated heap-owned identifier strings.
Retained RSS delta fell 4.46%; peak RSS varied +1.05%, within the comparator's
noise allowance. Non-buffered policies now scan without a candidate `Vec`.

## Allocation-count follow-up — 2026-09-03

The certification allocator exposed a separate lookup cost: every candidate
constructed an owned `(installation_id, core_id)` tuple only to query the worker
map. The existing Core index is now an allocation-free fast path, with an exact
scan bounded by 1,024 workers when installations share a Core ID. Across matched
complete Gateway runs, allocation operations fell from 7,439,239 to 810,640
(-89.10%) and requested bytes from 161,250,071 to 88,341,495 (-45.21%). p99
fell from 19,334 to 16,250 ns for round-robin, 8,000 to 6,750 ns for
least-inflight and 31,542 to 24,958 ns for affinity.
