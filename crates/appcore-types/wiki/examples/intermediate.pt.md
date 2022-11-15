# Compatibilidade de identidade distribuida

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Verifique se dois Cores podem se comunicar respeitando tenant, cluster,
protocolo, contrato do Runtime e capability.

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

`node_id`, `core_id` e `instance_id` podem diferir. Familia da aplicacao, sync
group e contrato do Runtime devem ser compativeis; a verificacao distribuida
tambem exige tenant, cluster, protocolo e capabilities anunciadas.
