# Degradacao explicita quando offline

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Coordene um heartbeat offline com uma politica explicita de degradacao. Chame a
funcao no executor async que ja pertence ao host.

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

O modo degradado e explicito. Writes dependentes de lideranca ainda devem parar
quando coordenacao ou uma lease valida estiver indisponivel.
