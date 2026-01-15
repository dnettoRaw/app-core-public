# Route a tenant capability to a worker

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Register one authenticated worker advertisement inside a tenant partition and
resolve the worker for an incoming capability.

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

Keep one registry per tenant partition. Connection authentication and tenant
binding must complete before an advertisement enters the registry.
