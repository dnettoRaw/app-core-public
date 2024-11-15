# Resolve a fenced mutating capability

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Resolve a local command only after enforcing its idempotency and service lease
requirements.

```rust
use appcore_capabilities::{
    CapabilityRegistry, CapabilityRequest, CapabilityResolver,
    CapabilityResponse, CapabilityResult, LocalCapabilityHandler,
};
use appcore_contracts::ServiceId;
use appcore_control_plane::{ServiceLeaderLease, StaticServiceLeadershipGuard};
use appcore_core::{
    AppFamily, AppId, CapabilityDescriptor, CapabilityMode, CapabilityName,
    CapabilityRequirements, CapabilityVisibility, CoreIdentity, NodeId,
    RuntimeContractVersion, RuntimeIdentity, SyncGroup,
};

struct CreateDocument {
    descriptor: CapabilityDescriptor,
}

impl LocalCapabilityHandler for CreateDocument {
    fn descriptor(&self) -> CapabilityDescriptor { self.descriptor.clone() }

    fn handle(&self, request: &CapabilityRequest) -> CapabilityResult<CapabilityResponse> {
        Ok(CapabilityResponse::accepted(request.payload.clone(), None))
    }
}

fn main() -> Result<(), String> {
    let identity = CoreIdentity::from_runtime_defaults(RuntimeIdentity {
        app_id: AppId::new("documents-app".to_string()).map_err(debug)?,
        app_family: AppFamily::new("documents".to_string()).map_err(debug)?,
        sync_group: SyncGroup::new("cluster-eu".to_string()).map_err(debug)?,
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("core-paris".to_string()).map_err(debug)?,
    })
    .map_err(debug)?;
    let name = CapabilityName::new("document.create").map_err(debug)?;
    let service_id = ServiceId::new("document.writer").map_err(debug)?;
    let descriptor = CapabilityDescriptor {
        name: name.clone(),
        version: "1".to_string(),
        mode: CapabilityMode::Command,
        visibility: CapabilityVisibility::Cluster,
        requirements: CapabilityRequirements {
            requires_leader: true,
            read_only: false,
            idempotency_required: true,
        },
    };
    let mut registry = CapabilityRegistry::new();
    registry
        .register_handler(CreateDocument { descriptor })
        .map_err(debug)?;
    let leadership = StaticServiceLeadershipGuard::new([ServiceLeaderLease {
        service_id: service_id.clone(),
        tenant_id: identity.tenant_id.clone(),
        cluster_id: identity.cluster_id.clone(),
        holder_core_id: identity.core_id.clone(),
        epoch: 9,
        acquired_at_ms: 1_700_000_000_000,
        expires_at_ms: 1_700_000_030_000,
    }]);
    let response = CapabilityResolver::new(registry)
        .handle_local(
            &identity,
            &service_id,
            &CapabilityRequest {
                request_id: "request-42".to_string(),
                capability: name,
                mode: CapabilityMode::Command,
                payload: br#"{"title":"Runtime notes"}"#.to_vec(),
                idempotency_key: Some("create-document-42".to_string()),
                trace: None,
            },
            Some(&leadership),
            1_700_000_010_000,
        )
        .map_err(debug)?;

    println!("accepted={}", response.accepted);
    Ok(())
}

fn debug(error: impl std::fmt::Debug) -> String { format!("{error:?}") }
```

Removing the idempotency key or using an expired/wrong-holder lease rejects the
request before the application handler runs.
