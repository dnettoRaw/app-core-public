# Router une capability tenant vers un worker

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Enregistrez l'annonce d'un worker authentifie dans la partition tenant, puis
resolvez le worker pour une capability entrante.

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

Gardez un registry par partition tenant. L'authentification de la connexion et
le binding du tenant doivent finir avant l'insertion de l'annonce.
