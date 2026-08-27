# SyncOutbox paging benchmark — 2026-08-26

Implementation commit: `c904e833c4cf973b5a9b91916119935c0bcb5da8`

The clean release-profile `appcore-dev cert bottlenecks` run used Rust 1.97.1
on macOS/aarch64. It queued 256 messages containing 32 KiB event fixtures and
measured 16 complete snapshots against 16 pages bounded to eight messages and
512 KiB.

| Read | Returned messages | Materialized bytes | p99 | Throughput |
|---|---:|---:|---:|---:|
| Complete compatibility snapshot | 256 | 30,021,820 | 1,404,417 ns | 1,627/s |
| Bounded page | 7 | 460,684 | 71,458 ns | 15,754/s |
| Payload-free stats | 0 | 0 payload bytes | 54,542 ns | 19,258/s |

The page stopped at the byte limit before cloning an eighth message. It reduced
materialized bytes by 98.46%. A future readiness timestamp hid the ordered
front, and an exact four-message receipt removed only that prefix. Every
subsystem passed and the complete process peaked at 244,752 KiB.

This is repository-local evidence, not a production workload or cross-platform
certification. V1 peer-wire fixtures remain unchanged.
