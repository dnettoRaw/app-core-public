# appcore-scheduler

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** bounded local task execution and explainable Core placement.

**Internal dependencies:** `appcore-contracts`, `appcore-core`.

**Primary API:** `Scheduler`, `SchedulerConfig`, `ScheduledTask`,
`TaskSchedule`, task callback/context/result, retry policy, handle and
snapshots; resource/placement requests, candidates, rejections, evaluations,
decisions and `PlacementEngine`.

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

**Maturity:** stable local RC profile; schedule state is process-local.
