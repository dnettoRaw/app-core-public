# Contrato de deployment em cluster

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Construa um Deployment Manifest de cluster com providers explicitos, control
plane HTTPS e referencia de segredo. Nenhum valor de credencial entra no
manifest.

```rust
use appcore_contracts::{
    ApplicationId, ContractResult, DeploymentManifestV1, InstallationId,
    NetworkConfig, ProviderConfig, ProviderId, RuntimeMode, SecretRef,
};

fn main() -> ContractResult<()> {
    let control_plane = ProviderConfig::new(ProviderId::new("vercel-neon")?)
        .with_endpoint("https://control.example.com")?
        .with_secret_ref(
            "auth_token",
            SecretRef::new("env:APPCORE_CONTROL_TOKEN")?,
        )?;
    let network = NetworkConfig::new(
        ProviderId::new("https")?,
        ProviderId::new("https")?,
    )
    .with_listen_address("127.0.0.1:9090")?;

    let deployment = DeploymentManifestV1::builder(
        InstallationId::new("notes-eu-1")?,
        ApplicationId::new("notes-app")?,
        RuntimeMode::Cluster,
        ProviderConfig::new(ProviderId::new("file")?),
        network,
    )
    .with_control_plane(control_plane)
    .with_peer_discovery(ProviderConfig::new(ProviderId::new("control-plane")?))
    .with_path("storage", "data/runtime")?
    .with_environment_secret(
        "APPCORE_CONTROL_TOKEN",
        SecretRef::new("env:APPCORE_CONTROL_TOKEN")?,
    )?
    .build()?;

    assert_eq!(deployment.mode(), RuntimeMode::Cluster);
    println!("installation={}", deployment.installation_id());
    Ok(())
}
```

O modo cluster falha sem providers de control plane e peer discovery. Nomes de
ambiente sensiveis aceitam apenas `SecretRef`, nunca valores literais.
