# Recuperar e encerrar um scheduler durável limitado

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Execute uma tarefa recuperável com limites explícitos de capacidade, lease e
polling, depois aguarde seu encerramento limpo. Use um owner ID diferente para
cada processo executado ao mesmo tempo.

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

Callbacks longos devem consultar `TaskContext::is_cancelled` e aplicar o epoch
de fencing na fronteira do efeito protegido. O recovery é at-least-once até o
commit do receipt. A policy de retry deve ser limitada e erros do callback não
podem conter secrets.
