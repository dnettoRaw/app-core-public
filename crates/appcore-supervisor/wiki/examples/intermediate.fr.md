# Supervision avec dependances

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Cet exemple enregistre deux ressources passives, valide leur graphe de
dependances, les demarre dans l'ordre, inspecte leur health et realise un
shutdown cooperatif.

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

Utilisez `ManagedThreadService` ou `CallbackManagedService` pour les ressources
reelles. Le Supervisor ne possede que les services du processus; systemd,
launchd, WinSW ou l'orchestrateur doit posseder le processus.
