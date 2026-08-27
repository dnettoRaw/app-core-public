# appcore-scheduler

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

The 1.5 alpha opt-in state contract retains only task identity, definition
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

**Maturity:** stable local RC profile; durable state is opt-in on the 1.5 alpha candidate.
