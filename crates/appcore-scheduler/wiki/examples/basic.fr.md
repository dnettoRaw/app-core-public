# Tache planifiee minimale

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Validez une tache locale recurrente sans demarrer l'infrastructure de workers.

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

La planification est locale au processus et bornee. Les workflows applicatifs
durables n'appartiennent pas a ce scheduler.
