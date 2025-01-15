# Verification minimale du leadership par service

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Protegez les writes avec une lease limitee a un service, tenant et cluster.

```rust
use appcore_contracts::ServiceId;
use appcore_control_plane::{
    LeadershipDecision, ServiceLeaderLease, ServiceLeadershipGuard,
    StaticServiceLeadershipGuard,
};
use appcore_core::{ClusterId, CoreId, TenantId};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let service_id = ServiceId::new("storage.writer")?;
    let tenant_id = TenantId::new("tenant-acme")
        .map_err(|error| format!("{error:?}"))?;
    let cluster_id = ClusterId::new("cluster-eu")
        .map_err(|error| format!("{error:?}"))?;
    let core_id = CoreId::new("core-paris")
        .map_err(|error| format!("{error:?}"))?;
    let guard = StaticServiceLeadershipGuard::new([ServiceLeaderLease {
        service_id: service_id.clone(),
        tenant_id: tenant_id.clone(),
        cluster_id: cluster_id.clone(),
        holder_core_id: core_id.clone(),
        epoch: 7,
        acquired_at_ms: 1_700_000_000_000,
        expires_at_ms: 1_700_000_030_000,
    }]);

    let decision = guard.check_service_write_permission(
        &service_id,
        &tenant_id,
        &cluster_id,
        &core_id,
        Some(7),
        1_700_000_010_000,
    );
    assert_eq!(decision, LeadershipDecision::Allowed);
    Ok(())
}
```

Reverifiez la lease juste avant chaque write protege. Une decision expiree,
stale ou avec un autre holder doit echouer de maniere fermee.
