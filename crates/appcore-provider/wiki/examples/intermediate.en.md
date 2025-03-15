# Explicit provider factory resolution

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Register one storage factory and resolve exactly the provider selected by a
validated Deployment Manifest. There is no implicit fallback.

```rust
use appcore_contracts::{
    ApplicationId, DeploymentManifestV1, InstallationId, NetworkConfig,
    ProviderConfig, ProviderId, RuntimeMode, SecretRef,
};
use appcore_provider::{
    DeploymentProviderPlan, ProviderContext, ProviderFactory, ProviderRegistry,
    ProviderResult, ProviderRole, ResolvedSecret, SecretProvider,
};
use std::error::Error;

struct NoSecrets;

impl SecretProvider for NoSecrets {
    fn resolve(&self, _reference: &SecretRef) -> ProviderResult<ResolvedSecret> {
        Err(appcore_provider::ProviderError::SecretUnavailable("disabled".into()))
    }
}

struct FileLabelFactory;

impl ProviderFactory<String> for FileLabelFactory {
    fn role(&self) -> ProviderRole { ProviderRole::Storage }
    fn provider_id(&self) -> &'static str { "file-label" }

    fn create(
        &self,
        config: &ProviderConfig,
        _context: &ProviderContext,
        _secrets: &dyn SecretProvider,
    ) -> ProviderResult<String> {
        Ok(format!("storage:{}", config.provider_id().as_str()))
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let selected = ProviderConfig::new(ProviderId::new("file-label")?);
    let manifest = DeploymentManifestV1::builder(
        InstallationId::new("notes-local")?,
        ApplicationId::new("notes-app")?,
        RuntimeMode::Standalone,
        selected,
        NetworkConfig::new(ProviderId::new("http")?, ProviderId::new("http")?),
    )
    .build()?;
    let plan = DeploymentProviderPlan::from_manifest(&manifest);

    let mut registry = ProviderRegistry::new();
    registry.register(FileLabelFactory)?;
    let storage = registry.create(
        ProviderRole::Storage,
        plan.storage(),
        plan.context(),
        &NoSecrets,
    )?;

    println!("{storage}");
    Ok(())
}
```

Use a separate registry per output interface. Duplicate ownership and unknown
provider IDs return typed errors instead of silently selecting another backend.
