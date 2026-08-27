# Rotear capability de tenant para um worker

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Registre o anuncio de um worker autenticado dentro da particao de tenant e
resolva o worker para uma capability recebida.

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

Mantenha um registry por particao de tenant. Autenticacao da conexao e binding
do tenant devem terminar antes de inserir o anuncio.
`FirstAvailable` permanece o default. Para planejamento least-inflight,
health-weighted ou affinity, chame `TenantState::select_worker` com um
`WorkerSelectionInput` limitado; o dispatch real revalida health e o limite de
64 rotas por worker antes de registrar o request pendente.

Para um Gateway do Runtime, leia o snapshot limitado sem adicionar endpoint de
métricas nem SDK de vendor ao processo de roteamento:

```rust
let snapshot = gateway_runtime.snapshot();
for series in &snapshot.telemetry.capabilities {
    println!(
        "capability={} requests={} p99_ns={}",
        series.capability, series.requests, series.latency_p99_ns
    );
}
```

Faça a conversão Prometheus/OpenTelemetry em código do deployment. Não adicione
atributos de tenant, request ou credencial ao traduzir o snapshot.
