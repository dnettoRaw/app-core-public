# Distributed identity compatibility

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Check whether two Cores may communicate under tenant, cluster, protocol,
Runtime-contract and capability constraints.

```rust
use appcore_types::{
    AppFamily, AppId, CapabilityName, CoreCompatibilityPolicy, CoreIdentity,
    NodeId, RuntimeContractVersion, RuntimeIdentity, RuntimeResult, SyncGroup,
};

fn runtime(node: &str) -> RuntimeResult<RuntimeIdentity> {
    Ok(RuntimeIdentity {
        app_id: AppId::new("notes-app")?,
        app_family: AppFamily::new("notes")?,
        sync_group: SyncGroup::new("primary")?,
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new(node)?,
    })
}

fn main() -> RuntimeResult<()> {
    let local = CoreIdentity::from_runtime_defaults(runtime("node-a")?)?;
    let peer = CoreIdentity::from_runtime_defaults(runtime("node-b")?)?;
    let capability = CapabilityName::new("notes.read")?;
    let policy = CoreCompatibilityPolicy {
        require_same_cluster: true,
        required_capability: Some(capability.clone()),
    };

    local.ensure_compatible(&peer, &policy, &[capability])?;
    println!("{} can call {}", local.core_id.as_str(), peer.core_id.as_str());
    Ok(())
}
```

`node_id`, `core_id` and `instance_id` may differ. Application family, sync
group and Runtime contract must remain compatible; distributed checks also
enforce tenant, cluster, protocol and advertised capabilities.
