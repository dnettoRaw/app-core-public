# Récupérer et arrêter un scheduler durable borné

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Exécutez une tâche récupérable avec des limites explicites de capacité, lease
et polling, puis joignez proprement le scheduler. Utilisez un owner ID distinct
pour chaque processus exécuté simultanément.

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

Les callbacks longs doivent consulter `TaskContext::is_cancelled` et appliquer
l'epoch de fencing à la frontière de l'effet protégé. La récupération reste
at-least-once jusqu'au commit du receipt. La policy de retry doit rester bornée
et les erreurs ne doivent jamais contenir de secrets.
