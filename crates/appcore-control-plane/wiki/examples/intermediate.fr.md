# Degradation explicite hors ligne

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Coordonnez un heartbeat offline avec une policy explicite de degradation.
Appelez cette fonction depuis l'executor async deja possede par le host.

```rust
use appcore_control_plane::{
    ControlPlaneCoordinator, HeartbeatPolicy, HeartbeatRequest,
    OfflineControlPlaneClient,
};
use appcore_core::{
    AppFamily, AppId, CoreIdentity, NodeId, RuntimeContractVersion,
    RuntimeIdentity, RuntimeOperationalMode, SyncGroup,
};
use std::error::Error;

async fn refresh_operational_mode() -> Result<RuntimeOperationalMode, Box<dyn Error>> {
    let identity = CoreIdentity::from_runtime_defaults(RuntimeIdentity {
        app_id: AppId::new("notes-app".to_string())
            .map_err(|error| format!("{error:?}"))?,
        app_family: AppFamily::new("notes".to_string())
            .map_err(|error| format!("{error:?}"))?,
        sync_group: SyncGroup::new("cluster-eu".to_string())
            .map_err(|error| format!("{error:?}"))?,
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("core-paris".to_string())
            .map_err(|error| format!("{error:?}"))?,
    })
    .map_err(|error| format!("{error:?}"))?;
    let coordinator = ControlPlaneCoordinator::new(
        OfflineControlPlaneClient,
        HeartbeatPolicy {
            allow_degraded_on_offline: true,
        },
    );
    let mode = coordinator
        .heartbeat_once(HeartbeatRequest {
            identity,
            operation_mode: RuntimeOperationalMode::ReadWrite,
            sent_at_ms: 1_700_000_000_000,
        })
        .await?;

    assert_eq!(mode, RuntimeOperationalMode::Degraded);
    Ok(mode)
}
```

Le mode degrade est explicite. Les writes dependant du leadership doivent
s'arreter si la coordination ou une lease valide devient indisponible.
