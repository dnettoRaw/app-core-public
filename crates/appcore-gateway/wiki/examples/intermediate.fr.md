# Router une capability tenant vers un worker

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Enregistrez l'annonce d'un worker authentifie dans la partition tenant, puis
resolvez le worker pour une capability entrante.

```rust
use appcore_contracts::InstallationId;
use appcore_gateway::{
    CapabilityRegistry, CapabilityResolver, WorkerConnectionKey, WorkerSelectionPolicy,
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

    let resolver = CapabilityResolver::with_policy(WorkerSelectionPolicy::RoundRobin);
    let selected = resolver
        .resolve(&capability, &registry)
        .ok_or_else(|| "no worker available".to_string())?;
    if selected.tenant_id != tenant_id {
        return Err("tenant partition mismatch".to_string());
    }

    println!("installation={}", selected.installation_id.as_str());
    registry.deregister(&worker);
    assert!(resolver.resolve(&capability, &registry).is_none());
    Ok(())
}

fn debug(error: impl std::fmt::Debug) -> String { format!("{error:?}") }
```

Gardez un registry par partition tenant. L'authentification de la connexion et
le binding du tenant doivent finir avant l'insertion de l'annonce.
`FirstAvailable` reste le défaut. Pour le planning least-inflight,
health-weighted ou affinity, appelez `TenantState::select_worker` avec un
`WorkerSelectionInput` borné ; le dispatch réel revérifie health et la limite
de 64 routes par worker avant d'enregistrer la request en attente.

Pour un Gateway possédé par le Runtime, lisez le snapshot borné sans ajouter
d'endpoint métrique ni SDK vendor au processus de routage :

```rust
let details = gateway_runtime.details();
for series in &details.telemetry.capabilities {
    println!(
        "capability={} requests={} p99_ns={}",
        series.capability, series.requests, series.latency_p99_ns
    );
}
```

Effectuez la conversion Prometheus/OpenTelemetry dans le code du déploiement.
N'ajoutez aucun attribut tenant, request ou credential lors de la traduction.
