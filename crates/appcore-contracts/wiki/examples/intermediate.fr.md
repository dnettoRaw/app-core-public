# Contrat de deploiement en cluster

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Construisez un Deployment Manifest de cluster avec des providers explicites,
un control plane HTTPS et une reference de secret. Aucune valeur d'identifiant
n'entre dans le manifest.

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

Le mode cluster echoue sans providers de control plane et de peer discovery.
Les noms d'environnement sensibles n'acceptent que `SecretRef`, jamais des
valeurs litterales.
