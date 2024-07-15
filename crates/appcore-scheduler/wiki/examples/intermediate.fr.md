# Executer et arreter un scheduler borne

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Executez une tache dans un scheduler avec des limites explicites de capacite,
concurrence et polling, puis joignez proprement le scheduler.

```rust
use appcore_scheduler::{
    RetryPolicy, ScheduledTask, Scheduler, SchedulerConfig, TaskCallback,
    TaskSchedule,
};
use std::sync::{mpsc, Arc};
use std::time::{Duration, SystemTime};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let scheduler = Scheduler::new(SchedulerConfig {
        max_tasks: 16,
        max_concurrent_tasks: 2,
        poll_interval: Duration::from_millis(10),
    })?;
    let (completed_tx, completed_rx) = mpsc::sync_channel(1);
    let callback: TaskCallback = Arc::new(move |context| {
        completed_tx
            .try_send((context.task_id().to_string(), context.attempt()))
            .map_err(|error| error.to_string())?;
        Ok(())
    });
    let handle = scheduler.schedule(
        ScheduledTask {
            id: "index.refresh".to_string(),
            schedule: TaskSchedule::Once {
                run_at: SystemTime::now() + Duration::from_millis(25),
            },
            retry: RetryPolicy::default(),
            priority: 20,
            trace: None,
        },
        callback,
    )?;

    let (task_id, attempt) = completed_rx.recv_timeout(Duration::from_secs(2))?;
    println!("task={task_id} attempt={attempt} handle={}", handle.id());
    scheduler.shutdown()?;
    Ok(())
}
```

Les callbacks longs doivent consulter `TaskContext::is_cancelled`. La policy de
retry doit rester bornee et les erreurs ne doivent jamais contenir de secrets.
