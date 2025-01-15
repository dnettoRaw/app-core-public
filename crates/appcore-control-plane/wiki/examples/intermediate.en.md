# Explicit degradation while offline

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Coordinate an offline heartbeat with an explicit degradation policy. Call this
function from the async executor already owned by the host.

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

Degraded mode is explicit. Leadership-dependent writes must still stop when
coordination or a valid service lease is unavailable.
