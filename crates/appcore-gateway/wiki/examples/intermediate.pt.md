# Rotear capability de tenant para um worker

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Registre o anuncio de um worker autenticado dentro da particao de tenant e
resolva o worker para uma capability recebida.

```rust
use appcore_contracts::InstallationId;
use appcore_gateway::{
    CapabilityRegistry, CapabilityResolver, WorkerConnectionKey,
};
use appcore_types::{CapabilityName, CoreId, TenantId};

fn main() -> Result<(), String> {
    let tenant_id = TenantId::new("tenant-acme").map_err(debug)?;
    let capability = CapabilityName::new("document.query").map_err(debug)?;
    let worker = WorkerConnectionKey {
        tenant_id: tenant_id.clone(),
        installation_id: InstallationId::new("notes-eu-1").map_err(debug)?,
        core_id: CoreId::new("worker-paris-1").map_err(debug)?,
    };
    let mut registry = CapabilityRegistry::new();
    registry.register(worker.clone(), vec![capability.clone()]);

    let selected = CapabilityResolver::new()
        .resolve(&capability, &registry)
        .ok_or_else(|| "no worker available".to_string())?;
    if selected.tenant_id != tenant_id {
        return Err("tenant partition mismatch".to_string());
    }

    println!("installation={}", selected.installation_id.as_str());
    registry.deregister(&worker);
    assert!(CapabilityResolver::new().resolve(&capability, &registry).is_none());
    Ok(())
}

fn debug(error: impl std::fmt::Debug) -> String { format!("{error:?}") }
```

Mantenha um registry por particao de tenant. Autenticacao da conexao e binding
do tenant devem terminar antes de inserir o anuncio.
