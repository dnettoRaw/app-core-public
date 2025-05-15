# Construire le client de control plane du cluster

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Resolvez le token reference au deploiement et creez le provider depuis un
contexte cluster valide. La construction n'envoie aucune requete.

```rust
use appcore_contracts::{
    ApplicationId, DeploymentManifestV1, InstallationId, NetworkConfig,
    ProviderConfig, ProviderId, RuntimeMode, SecretRef,
};
use appcore_provider::{
    ProviderContext, ProviderError, ProviderFactory, ProviderResult,
    ResolvedSecret, SecretProvider,
};
use appcore_provider_vercel_neon::{
    VercelNeonControlPlaneFactory, AUTH_TOKEN_SECRET, VERCEL_NEON_PROVIDER_ID,
};
use std::error::Error;

struct EnvironmentSecrets;

impl SecretProvider for EnvironmentSecrets {
    fn resolve(&self, reference: &SecretRef) -> ProviderResult<ResolvedSecret> {
        let name = reference.as_str().strip_prefix("env:").ok_or_else(|| {
            ProviderError::SecretUnavailable("unsupported secret reference".to_string())
        })?;
        let value = std::env::var(name).map_err(|_| {
            ProviderError::SecretUnavailable("environment secret unavailable".to_string())
        })?;
        ResolvedSecret::new(value)
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let manifest = DeploymentManifestV1::builder(
        InstallationId::new("notes-cluster-eu")?,
        ApplicationId::new("notes-app")?,
        RuntimeMode::Cluster,
        ProviderConfig::new(ProviderId::new("file")?),
        NetworkConfig::new(ProviderId::new("https")?, ProviderId::new("https")?),
    )
    .with_control_plane(ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID)?))
    .with_peer_discovery(ProviderConfig::new(ProviderId::new("control-plane")?))
    .build()?;
    let context = ProviderContext::from_manifest(&manifest);
    let config = ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID)?)
        .with_endpoint("https://control.example.com")?
        .with_secret_ref(
            AUTH_TOKEN_SECRET,
            SecretRef::new("env:APPCORE_CONTROL_TOKEN")?,
        )?
        .with_setting("max_attempts", "3")?;

    let factory = VercelNeonControlPlaneFactory;
    let _provider = factory.create(&config, &context, &EnvironmentSecrets)?;
    println!("created provider={}", factory.provider_id());
    Ok(())
}
```

La construction refuse le mode standalone, HTTP simple, une reference absente
et des limites de retry invalides. Gardez la variable hors des manifests et logs.
