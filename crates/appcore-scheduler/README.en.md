# appcore-scheduler

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Bounded one-shot, interval and cron execution plus deterministic Core
placement.

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

```bash
cargo test -p appcore-scheduler
```
