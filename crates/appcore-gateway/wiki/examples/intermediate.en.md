# Route a tenant capability to a worker

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Register one authenticated worker advertisement inside a tenant partition and
resolve the worker for an incoming capability.

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

Keep one registry per tenant partition. Connection authentication and tenant
binding must complete before an advertisement enters the registry.
`FirstAvailable` remains the default. For least-inflight, health-weighted or
affinity planning, call `TenantState::select_worker` with a bounded
`WorkerSelectionInput`; actual dispatch rechecks health and the 64-route
per-worker limit before registering the pending request.

For a Runtime-owned Gateway, pull the bounded telemetry snapshot without
adding a metrics endpoint or vendor SDK to the routing process:

```rust
let snapshot = gateway_runtime.snapshot();
for series in &snapshot.telemetry.capabilities {
    println!(
        "capability={} requests={} p99_ns={}",
        series.capability, series.requests, series.latency_p99_ns
    );
}
```

Run Prometheus/OpenTelemetry conversion in deployment-owned code. Do not add
tenant, request or credential attributes while translating the snapshot.
