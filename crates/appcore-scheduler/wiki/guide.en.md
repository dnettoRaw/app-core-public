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

**Maturity:** stable local RC profile; schedule state is process-local.
