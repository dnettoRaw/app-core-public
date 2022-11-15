# Compatibilite d'identite distribuee

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Verifiez si deux Cores peuvent communiquer sous les contraintes de tenant,
cluster, protocole, contrat Runtime et capability.

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

`node_id`, `core_id` et `instance_id` peuvent differer. La famille applicative,
le sync group et le contrat Runtime doivent rester compatibles; le controle
distribue impose aussi tenant, cluster, protocole et capabilities annoncees.
