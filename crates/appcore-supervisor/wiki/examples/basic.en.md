# Minimal managed-service policy

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Define a validated service and a bounded restart policy without starting any
threads.

```rust
use appcore_supervisor::{ManagedResource, RestartPolicy, ServiceDescriptor};
use std::error::Error;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let policy = RestartPolicy::bounded(3, Duration::from_secs(60))?
        .with_backoff(Duration::from_millis(200), Duration::from_millis(50))
        .with_shutdown_timeout(Duration::from_secs(5));
    let service = ServiceDescriptor::new("metrics", ManagedResource::Metrics, policy)?;

    println!("{}: {:?}", service.name(), service.restart_policy().mode);
    Ok(())
}
```

The descriptor is immutable after construction. Invalid names, zero restart
budgets and zero shutdown deadlines fail before lifecycle work begins.
