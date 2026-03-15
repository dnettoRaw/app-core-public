// =============================================================================
//        #######
//     ###       ###     F: application_context.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 13:45:20 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Validated installation bindings exposed to hosted application code.

use crate::bootstrap::BootstrapError;
use crate::providers::DeploymentSecretResolver;
use appcore_contracts::{
    DeploymentManifestV1, EnvironmentBinding, NetworkConfig, ProviderConfig, VolumeMount,
};
use appcore_provider::{ResolvedSecret, SecretProvider};
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

/// Immutable deployment bindings prepared before application registration.
pub struct DeploymentContext {
    paths: BTreeMap<String, PathBuf>,
    volumes: Vec<ResolvedVolumeMount>,
    adapters: BTreeMap<String, ProviderConfig>,
    network: NetworkConfig,
    environment: BTreeMap<String, DeploymentEnvironmentValue>,
}

/// A volume whose source is resolved relative to the deployment manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedVolumeMount {
    name: String,
    source: PathBuf,
    target: PathBuf,
    read_only: bool,
}

/// Non-sensitive literal or redacted resolved secret.
pub enum DeploymentEnvironmentValue {
    /// Non-sensitive literal stored in the Deployment Manifest.
    Literal(String),
    /// Secret resolved only at bootstrap and zeroized on drop.
    Secret(ResolvedSecret),
}

impl DeploymentContext {
    pub(crate) fn resolve(
        manifest: &DeploymentManifestV1,
        deployment_path: &Path,
    ) -> Result<Self, BootstrapError> {
        let directory = deployment_path.parent().unwrap_or_else(|| Path::new("."));
        let paths = resolve_paths(manifest, directory)?;
        let volumes = manifest
            .volumes()
            .iter()
            .map(|volume| resolve_volume(volume, directory))
            .collect::<Result<Vec<_>, _>>()?;
        let secrets = DeploymentSecretResolver::from_manifest(manifest)?;
        let environment = resolve_environment(manifest, &secrets)?;
        Ok(Self {
            paths,
            volumes,
            adapters: manifest.adapters().clone(),
            network: manifest.network().clone(),
            environment,
        })
    }

    /// Returns a named installation path.
    pub fn path(&self, name: &str) -> Option<&Path> {
        self.paths.get(name).map(PathBuf::as_path)
    }

    /// Returns validated volume bindings.
    pub fn volumes(&self) -> &[ResolvedVolumeMount] {
        &self.volumes
    }

    /// Returns named application adapter selections.
    pub fn adapters(&self) -> &BTreeMap<String, ProviderConfig> {
        &self.adapters
    }

    /// Returns the validated installation network contract.
    pub fn network(&self) -> &NetworkConfig {
        &self.network
    }

    /// Returns one resolved environment binding.
    pub fn environment(&self, name: &str) -> Option<&DeploymentEnvironmentValue> {
        self.environment.get(name)
    }
}

impl ResolvedVolumeMount {
    /// Returns the logical volume name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the normalized installation-owned source.
    pub fn source(&self) -> &Path {
        &self.source
    }

    /// Returns the application-visible target.
    pub fn target(&self) -> &Path {
        &self.target
    }

    /// Reports whether the application must treat the volume as read-only.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }
}

impl DeploymentEnvironmentValue {
    /// Borrows the value for immediate application configuration.
    pub fn expose(&self) -> &str {
        match self {
            Self::Literal(value) => value,
            Self::Secret(value) => value.expose(),
        }
    }

    /// Reports whether the value came from a secret reference.
    pub fn is_secret(&self) -> bool {
        matches!(self, Self::Secret(_))
    }
}

impl std::fmt::Debug for DeploymentEnvironmentValue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Literal(value) => formatter.debug_tuple("Literal").field(value).finish(),
            Self::Secret(_) => formatter.write_str("Secret(REDACTED)"),
        }
    }
}

fn resolve_paths(
    manifest: &DeploymentManifestV1,
    directory: &Path,
) -> Result<BTreeMap<String, PathBuf>, BootstrapError> {
    manifest
        .paths()
        .iter()
        .map(|(name, path)| {
            normalize_path(directory, path)
                .map(|resolved| (name.clone(), resolved))
                .map_err(|reason| {
                    BootstrapError::Runtime(format!(
                        "invalid deployment path binding '{name}': {reason}"
                    ))
                })
        })
        .collect()
}

fn resolve_volume(
    volume: &VolumeMount,
    directory: &Path,
) -> Result<ResolvedVolumeMount, BootstrapError> {
    let source = normalize_path(directory, volume.source()).map_err(|reason| {
        BootstrapError::Runtime(format!(
            "invalid volume source '{}': {reason}",
            volume.name()
        ))
    })?;
    let target = normalize_target(volume.target()).map_err(|reason| {
        BootstrapError::Runtime(format!(
            "invalid volume target '{}': {reason}",
            volume.name()
        ))
    })?;
    Ok(ResolvedVolumeMount {
        name: volume.name().to_string(),
        source,
        target,
        read_only: volume.is_read_only(),
    })
}

fn resolve_environment(
    manifest: &DeploymentManifestV1,
    secrets: &DeploymentSecretResolver,
) -> Result<BTreeMap<String, DeploymentEnvironmentValue>, BootstrapError> {
    manifest
        .environment()
        .iter()
        .map(|(name, binding)| {
            let value = match binding {
                EnvironmentBinding::Literal(value) => {
                    DeploymentEnvironmentValue::Literal(value.clone())
                }
                EnvironmentBinding::Secret(reference) => DeploymentEnvironmentValue::Secret(
                    secrets
                        .resolve(reference)
                        .map_err(|error| BootstrapError::Runtime(error.to_string()))?,
                ),
            };
            Ok((name.clone(), value))
        })
        .collect()
}

fn normalize_path(directory: &Path, value: &str) -> Result<PathBuf, &'static str> {
    let path = Path::new(value);
    reject_parent_components(path)?;
    if path.is_absolute() {
        Ok(path.to_path_buf())
    } else {
        Ok(directory.join(path))
    }
}

fn normalize_target(value: &str) -> Result<PathBuf, &'static str> {
    let path = Path::new(value);
    reject_parent_components(path)?;
    if !path.is_absolute() {
        return Err("volume target must be absolute");
    }
    Ok(path.to_path_buf())
}

fn reject_parent_components(path: &Path) -> Result<(), &'static str> {
    if path
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err("parent traversal is forbidden");
    }
    Ok(())
}
