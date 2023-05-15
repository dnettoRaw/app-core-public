# Cluster deployment contract

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Build a cluster Deployment Manifest with explicit providers, an HTTPS control
plane and a secret reference. No credential value enters the manifest.

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

Cluster mode fails validation without both control-plane and peer-discovery
providers. Sensitive environment names accept only `SecretRef`, never literal
values.
