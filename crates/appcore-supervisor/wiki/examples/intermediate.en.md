# Dependency-aware supervision

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

This example registers two passive resources, validates their dependency graph,
starts them in order, inspects health and performs cooperative shutdown.

```rust
use appcore_supervisor::{
    DependencyRequirement, ManagedResource, PassiveManagedService, RestartPolicy,
    ServiceDescriptor, Supervisor,
};
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let storage = ServiceDescriptor::new(
        "storage",
        ManagedResource::Worker,
        RestartPolicy::never(),
    )?;
    let api = ServiceDescriptor::new(
        "api",
        ManagedResource::Http,
        RestartPolicy::bounded(3, Duration::from_secs(300))?,
    )?
    .with_dependency_requirement("storage", DependencyRequirement::Healthy)?;

    let supervisor = Supervisor::new();
    supervisor.register(Arc::new(PassiveManagedService::new(storage)))?;
    supervisor.register(Arc::new(PassiveManagedService::new(api)))?;

    assert_eq!(supervisor.validate()?, ["storage", "api"]);
    supervisor.start_all()?;
    for service in supervisor.snapshots() {
        println!("{}: {:?}", service.name, service.health);
    }
    supervisor.shutdown(1_000)?;
    Ok(())
}
```

Use `ManagedThreadService` or `CallbackManagedService` for real resources. The
Supervisor owns in-process services only; systemd, launchd, WinSW or an
orchestrator must own the process itself.
