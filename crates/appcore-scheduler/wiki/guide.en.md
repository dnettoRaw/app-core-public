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

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** bounded local task execution and explainable Core placement.

**Internal dependencies:** `appcore-contracts`, `appcore-core`.

**Primary API:** `Scheduler`, `SchedulerConfig`, `ScheduledTask`,
`TaskSchedule`, task callback/context/result, retry policy, handle and
snapshots; `DurableSchedulerConfigV1`, `SchedulerStateProvider`, memory/file
providers, claims and receipts V1; resource/placement requests, candidates,
rejections, evaluations, decisions and `PlacementEngine`.

Use it for Runtime or manifest-declared local work with explicit limits,
cancellation and shutdown. It is not a durable workflow engine or distributed
queue.

Shutdown closes admission while holding scheduler state, and deadline
arithmetic is checked. Unrepresentable one-shot, interval or retry times return
`InvalidSchedule` or remove the exhausted task instead of panicking.

The scheduler creates one fixed pool, limited by `max_concurrent_tasks`, and a
queue bounded to twice that effective worker count or `max_tasks`. When both
dispatch slots and the queue are occupied, later due tasks remain in the
registry without consuming an attempt. Observe pressure with
`worker_thread_count`, `queued_task_count` and `queue_saturation_count`.
Shutdown stops admission and drains already accepted callbacks with
`TaskContext::is_cancelled()` set. Callbacks are not forcibly terminated or
timed out because Rust threads cannot be safely preempted.

`SchedulerConfig` rejects values above `MAX_SCHEDULER_WORKERS` (64) or
`MAX_SCHEDULER_TASKS` (65,536). Every coordinator and callback worker has an
explicit 1 MiB stack, so configuration cannot reserve an unbounded thread set.

The due-task scan uses a bounded max-heap no larger than the currently
available dispatch slots. It preserves descending priority, earlier deadline
and registration order, and clones only selected task IDs. Because the queue
holds at most twice the effective worker count, the global maximum is 128
candidate records even when all 65,536 registered tasks are due.

The `1.0.2-rc` opt-in state contract retains only task identity, definition
hash, next run, attempts, misfire policy, current claim, fencing epoch and last
receipt. A confirmed one-shot receipt suppresses execution after restart. An
unreceipted expired claim is at-least-once recovery: callback effects must use
the exposed fencing epoch or their own idempotency boundary. The process-local
reference provider proves bounded two-owner claims. Configure
`Scheduler::with_state_provider`, then register only selected work with
`schedule_durable`; regular `schedule` calls remain ephemeral. The file
provider persists the same contract with same-process and interprocess locks, a
checksummed bounded V1 snapshot and atomic replacement. Callbacks must apply
`TaskContext::fencing_epoch` at their protected effect boundary when competing
owners are possible. See the
[V1 decision](../../../release/scheduler-state-provider-v1.md).

File-state I/O is bounded before allocation. Loading decodes from a reader
capped at 4 MiB; saving borrows the ordered records, computes the checksum by
streaming the exact JSON array and writes the complete snapshot through a fixed
64 KiB buffer. Atomic replacement and the V1 checksum remain unchanged. The
crate benchmark validates a maximum 1,024-record snapshot and reports its
idle/workload/retained memory phases.

Load validation borrows task, definition, owner and claim fields, while ordering
is compared with the last converted record. A maximum unclaimed snapshot avoids
3,072 temporary string allocations without changing V1 validation or bytes.

**Maturity:** current RC profile; durable state is opt-in.
