# Tarefa agendada minima

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Valide uma tarefa local recorrente sem iniciar infraestrutura de workers.

```rust
use appcore_scheduler::{RetryPolicy, ScheduledTask, TaskSchedule};
use std::time::Duration;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let task = ScheduledTask {
        id: "cache.refresh".to_string(),
        schedule: TaskSchedule::Interval {
            every: Duration::from_secs(60),
            start_at: None,
        },
        retry: RetryPolicy::default(),
        priority: 10,
        trace: None,
    };

    task.validate()?;
    println!("task={} is valid", task.id);
    Ok(())
}
```

O agendamento e local ao processo e limitado. Workflows duraveis da aplicacao
nao pertencem a este scheduler.
