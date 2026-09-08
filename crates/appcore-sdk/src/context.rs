// =============================================================================
//        #######
//     ###       ###     F: context.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Validated deployment bindings visible to application business code.
//!
//! The deployment integration resolves paths and secret references before
//! constructing this immutable value. Applications may read a binding but
//! cannot mutate installation policy or recover secret references from it.

use appcore_contracts::{NetworkConfig, ProviderConfig};
use appcore_provider::ResolvedSecret;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Immutable installation bindings prepared by deployment integration.
pub struct DeploymentContext {
    paths: BTreeMap<String, PathBuf>,
    volumes: Vec<ResolvedVolumeMount>,
    adapters: BTreeMap<String, ProviderConfig>,
    network: NetworkConfig,
    environment: BTreeMap<String, DeploymentEnvironmentValue>,
}

/// A validated application-visible volume mount.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedVolumeMount {
    name: String,
    source: PathBuf,
    target: PathBuf,
    read_only: bool,
}

/// A literal deployment value or a secret retained by its zeroizing owner.
pub enum DeploymentEnvironmentValue {
    /// A non-sensitive literal from the deployment manifest.
    Literal(String),
    /// A secret resolved by the selected provider and redacted in `Debug`.
    Secret(ResolvedSecret),
}

impl DeploymentContext {
    /// Builds a context from values already validated and resolved by a host.
    ///
    /// This constructor exists for deployment integrations and tests. Applications
    /// normally receive this value through [`crate::Application::configure`].
    #[doc(hidden)]
    pub fn from_resolved(
        paths: BTreeMap<String, PathBuf>,
        volumes: Vec<ResolvedVolumeMount>,
        adapters: BTreeMap<String, ProviderConfig>,
        network: NetworkConfig,
        environment: BTreeMap<String, DeploymentEnvironmentValue>,
    ) -> Self {
        Self {
            paths,
            volumes,
            adapters,
            network,
            environment,
        }
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
    /// Creates a volume mount from normalized host-owned paths.
    #[doc(hidden)]
    pub fn new(name: String, source: PathBuf, target: PathBuf, read_only: bool) -> Self {
        Self {
            name,
            source,
            target,
            read_only,
        }
    }

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
    /// Borrows a value for immediate application configuration.
    pub fn expose(&self) -> &str {
        match self {
            Self::Literal(value) => value,
            Self::Secret(value) => value.expose(),
        }
    }

    /// Reports whether the value originated from a secret reference.
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
