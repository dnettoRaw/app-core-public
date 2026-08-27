# Recover and stop a bounded durable scheduler

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Run one recoverable task with explicit capacity, lease and poll bounds, then
join the scheduler cleanly. Supply a different owner ID for every concurrently
running process.

```rust
use appcore_scheduler::{
    DurableSchedulerConfigV1, DurableTaskMisfirePolicyV1,
    FileSchedulerStateProvider, RetryPolicy, ScheduledTask, Scheduler,
    SchedulerConfig, TaskCallback, TaskSchedule,
};
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scheduler = Scheduler::with_state_provider(
        SchedulerConfig {
            max_tasks: 16,
            max_concurrent_tasks: 2,
            poll_interval: Duration::from_millis(10),
        },
        DurableSchedulerConfigV1::new(
            "runtime-node-a",
            Duration::from_secs(30),
            Duration::from_secs(2),
        )?,
        Arc::new(FileSchedulerStateProvider::new(
            "runtime-data/scheduler-v1.json",
        )?),
    )?;
    let (completed_tx, completed_rx) = mpsc::sync_channel(1);
    let callback: TaskCallback = Arc::new(move |context| {
        completed_tx
            .try_send((
                context.task_id().to_string(),
                context.attempt(),
                context.fencing_epoch(),
            ))
            .map_err(|error| error.to_string())?;
        Ok(())
    });
    let handle = scheduler.schedule_durable(
        ScheduledTask {
            id: "index.refresh".to_string(),
            schedule: TaskSchedule::Once {
                run_at: SystemTime::now() + Duration::from_millis(25),
            },
            retry: RetryPolicy::default(),
            priority: 20,
            trace: None,
        },
        DurableTaskMisfirePolicyV1::FireOnce,
        callback,
    )?;

    let (task_id, attempt, epoch) = completed_rx.recv_timeout(Duration::from_secs(2))?;
    println!("task={task_id} attempt={attempt} epoch={epoch:?} handle={}", handle.id());
    scheduler.shutdown()?;
    Ok(())
}
```

Callbacks should check `TaskContext::is_cancelled` during longer work and apply
the fencing epoch at the protected effect boundary. Recovery is at-least-once
until the receipt commits. A retry policy stays bounded and callback errors
must not include secrets.
