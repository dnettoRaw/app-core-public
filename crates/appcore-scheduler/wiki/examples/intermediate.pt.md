# Executar e encerrar um scheduler limitado

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Execute uma tarefa em um scheduler com limites explicitos de capacidade,
concorrencia e polling, depois aguarde seu encerramento limpo.

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

Callbacks longos devem consultar `TaskContext::is_cancelled`. A politica de
retry deve ser limitada e erros do callback nao podem conter secrets.
