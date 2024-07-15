# Minimal scheduled task

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Validate a local recurring task without starting worker infrastructure.

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

Scheduling is process-local and bounded. Durable application workflows do not
belong in this scheduler.
