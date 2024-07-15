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

```bash
cargo test -p appcore-scheduler
```
