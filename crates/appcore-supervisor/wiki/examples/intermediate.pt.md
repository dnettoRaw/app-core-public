# Supervisao com dependencias

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Este exemplo registra dois recursos passivos, valida o grafo de dependencias,
inicia na ordem correta, inspeciona health e executa shutdown cooperativo.

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

Use `ManagedThreadService` ou `CallbackManagedService` para recursos reais. O
Supervisor possui apenas servicos dentro do processo; systemd, launchd, WinSW
ou o orquestrador deve possuir o processo.
