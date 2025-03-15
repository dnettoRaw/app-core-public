# Resolution explicite d'une factory

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Enregistrez une factory de storage puis resolvez exactement le provider choisi
par un Deployment Manifest valide. Il n'existe aucun fallback implicite.

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

Utilisez un registry distinct par interface de sortie. Une ownership dupliquee
ou un ID inconnu renvoie une erreur typee au lieu de choisir un autre backend.
