# Resolucao explicita de factory

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Registre uma factory de storage e resolva exatamente o provider selecionado por
um Deployment Manifest validado. Nao existe fallback implicito.

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

Use um registry separado por interface de saida. Ownership duplicado e IDs
desconhecidos retornam erros tipados em vez de escolher outro backend.
