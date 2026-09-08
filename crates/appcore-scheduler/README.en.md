# appcore-scheduler

`shutdown_with_timeout(duration)` closes admission and requests cooperative
cancellation. `Ok(true)` means the coordinator and workers finished;
`Ok(false)` means they remain live and must not be replaced. Calls may be
repeated to observe completion. `shutdown()` uses a five-second wait budget
and returns `SchedulerError::Shutdown` on incomplete teardown. `Drop` requests
shutdown without waiting and detaches unfinished threads; their memory and
external effects can remain until callbacks/providers return. This is not
thread termination or a hard real-time deadline. Deployment must quarantine
an incomplete scheduler and use process isolation for untrusted callbacks.

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Bounded one-shot, interval and cron execution plus deterministic Core
placement.

**Responsibility:** bounded local execution and explainable Core placement.

**Internal dependencies:** `appcore-contracts`, `appcore-core`.

**Main API:** `Scheduler`, `SchedulerConfig`, `ScheduledTask`, `TaskSchedule`,
callback/context/result, retry policy, handle and snapshots;
`DurableSchedulerConfigV1`, `SchedulerStateProvider`, memory/file providers,
V1 claims and receipts; resource requests, candidates, rejections, evaluations,
decisions and `PlacementEngine`.

Tasks have explicit limits, retry, cancellation and shutdown. This is a local
scheduler, not a durable business workflow or distributed queue.

Admission is closed atomically with shutdown. Clock arithmetic for one-shot,
interval and retry scheduling is checked and reports `InvalidSchedule` instead
of panicking on an unrepresentable deadline.

Callbacks run on a fixed worker pool. The pool never exceeds
`max_concurrent_tasks`, and its internal queue is bounded to twice the worker
count or `max_tasks`, whichever is smaller. Excess due work remains scheduled
without consuming a retry; `queued_task_count` and `queue_saturation_count`
make pressure observable. Shutdown drains accepted callbacks with cancellation
set in `TaskContext`; execution has no unsafe preemptive timeout, so long
callbacks must cooperate through `is_cancelled`.

Configuration rejects more than `MAX_SCHEDULER_WORKERS` (64) callback threads
or `MAX_SCHEDULER_TASKS` (65,536) registered tasks. Coordinator and callback
threads use explicit 1 MiB stacks.

Each due-task scan retains only the best candidates that fit the currently
available dispatch slots. The bounded max-heap preserves descending priority,
earlier deadline and registration order, while cloning IDs only for retained
candidates. At the configured maximum, 65,536 due tasks therefore retain no
more than 128 candidate records instead of materializing the complete set.

The `1.0.2-rc` candidate provides opt-in `SchedulerStateProvider` V1 recovery.
Start it with `Scheduler::with_state_provider`, then use `schedule_durable` for
selected tasks. It persists next run, attempts and receipts, renews bounded
claims and exposes the monotonic fencing epoch to callbacks. `FireOnce` and
`Skip` are explicit misfire policies. `Scheduler::new` and `schedule` remain
process-local and offline. The file provider uses a bounded checksummed V1
snapshot, same-process and interprocess locks, atomic replacement and directory
sync. Recovery is at-least-once until the receipt commits.

The file provider decodes through a reader capped at 4 MiB. Save and checksum
borrow the recovered records, hash incrementally and serialize directly through
a fixed 64 KiB buffer into the exclusive temporary file. It preserves the
exact V1 bytes without retaining a file buffer, a second DTO list and an
encoded JSON copy at the same time.

Load validation also borrows task, definition, owner and claim fields and
checks ordering against the last converted record. A maximum unclaimed
snapshot therefore avoids 3,072 temporary string allocations while preserving
the exact V1 checks and bytes.

**Maturity:** stable local RC profile; scheduling remains process-local.

```bash
cargo test -p appcore-scheduler
```

## Stable documentation

Stable ID: **ACR-014**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-014). This permanent ID
remains valid if the wiki page moves.
