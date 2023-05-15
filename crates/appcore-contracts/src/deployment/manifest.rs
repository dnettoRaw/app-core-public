// =============================================================================
//        #######
//     ###       ###     F: manifest.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Contract describing one installation of an application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "DeploymentManifestData")]
pub struct DeploymentManifestV1 {
    manifest_version: u16,
    installation_id: InstallationId,
    application_id: ApplicationId,
    mode: RuntimeMode,
    supervisor: DeploymentSupervisorConfig,
    control_plane: Option<ProviderConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    coordination_store: Option<ProviderConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    secret_provider: Option<ProviderConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    job_provider: Option<ProviderConfig>,
    secrets: BTreeMap<String, SecretRef>,
    paths: BTreeMap<String, String>,
    volumes: Vec<VolumeMount>,
    storage: ProviderConfig,
    database: Option<ProviderConfig>,
    update_provider: Option<ProviderConfig>,
    network: NetworkConfig,
    peer_discovery: Option<ProviderConfig>,
    adapters: BTreeMap<String, ProviderConfig>,
    environment: BTreeMap<String, EnvironmentBinding>,
}

#[derive(Deserialize)]
struct DeploymentManifestData {
    manifest_version: u16,
    installation_id: InstallationId,
    application_id: ApplicationId,
    mode: RuntimeMode,
    #[serde(default)]
    supervisor: DeploymentSupervisorConfig,
    control_plane: Option<ProviderConfig>,
    coordination_store: Option<ProviderConfig>,
    secret_provider: Option<ProviderConfig>,
    job_provider: Option<ProviderConfig>,
    secrets: BTreeMap<String, SecretRef>,
    paths: BTreeMap<String, String>,
    volumes: Vec<VolumeMount>,
    storage: ProviderConfig,
    database: Option<ProviderConfig>,
    update_provider: Option<ProviderConfig>,
    network: NetworkConfig,
    peer_discovery: Option<ProviderConfig>,
    adapters: BTreeMap<String, ProviderConfig>,
    environment: BTreeMap<String, EnvironmentBinding>,
}

/// Builder that validates a deployment only after all providers are selected.
#[derive(Debug, Clone)]
pub struct DeploymentManifestBuilder {
    manifest: DeploymentManifestV1,
}

impl DeploymentManifestBuilder {
    /// Starts a deployment with required installation, storage and network choices.
    pub fn new(
        installation_id: InstallationId,
        application_id: ApplicationId,
        mode: RuntimeMode,
        storage: ProviderConfig,
        network: NetworkConfig,
    ) -> Self {
        Self {
            manifest: DeploymentManifestV1 {
                manifest_version: DEPLOYMENT_MANIFEST_VERSION,
                installation_id,
                application_id,
                mode,
                supervisor: DeploymentSupervisorConfig::default(),
                control_plane: None,
                coordination_store: None,
                secret_provider: None,
                job_provider: None,
                secrets: BTreeMap::new(),
                paths: BTreeMap::new(),
                volumes: Vec::new(),
                storage,
                database: None,
                update_provider: None,
                network,
                peer_discovery: None,
                adapters: BTreeMap::new(),
                environment: BTreeMap::new(),
            },
        }
    }

    /// Selects the cluster control-plane provider.
    pub fn with_control_plane(mut self, provider: ProviderConfig) -> Self {
        self.manifest.control_plane = Some(provider);
        self
    }

    /// Replaces installation-owned supervisor settings.
    pub fn with_supervisor(mut self, supervisor: DeploymentSupervisorConfig) -> Self {
        self.manifest.supervisor = supervisor;
        self
    }

    /// Selects an optional coordination-store provider.
    pub fn with_coordination_store(mut self, provider: ProviderConfig) -> Self {
        self.manifest.coordination_store = Some(provider);
        self
    }

    /// Selects the provider that resolves installation secret references.
    pub fn with_secret_provider(mut self, provider: ProviderConfig) -> Self {
        self.manifest.secret_provider = Some(provider);
        self
    }

    /// Selects an optional durable job provider.
    pub fn with_job_provider(mut self, provider: ProviderConfig) -> Self {
        self.manifest.job_provider = Some(provider);
        self
    }

    /// Adds an installation-level secret reference.
    pub fn with_secret(
        mut self,
        name: impl Into<String>,
        secret: SecretRef,
    ) -> ContractResult<Self> {
        let name = name.into();
        validate_text("deployment.secret.name", &name, 128)?;
        self.manifest.secrets.insert(name, secret);
        Ok(self)
    }

    /// Adds a named installation path.
    pub fn with_path(
        mut self,
        name: impl Into<String>,
        path: impl Into<String>,
    ) -> ContractResult<Self> {
        let name = name.into();
        let path = path.into();
        validate_text("deployment.path.name", &name, 128)?;
        validate_text("deployment.path", &path, 2_048)?;
        self.manifest.paths.insert(name, path);
        Ok(self)
    }

    /// Adds a volume mount.
    pub fn with_volume(mut self, volume: VolumeMount) -> Self {
        self.manifest.volumes.push(volume);
        self
    }

    /// Selects an optional application database provider.
    pub fn with_database(mut self, provider: ProviderConfig) -> Self {
        self.manifest.database = Some(provider);
        self
    }

    /// Selects an optional update provider.
    pub fn with_update_provider(mut self, provider: ProviderConfig) -> Self {
        self.manifest.update_provider = Some(provider);
        self
    }

    /// Selects the cluster peer-discovery provider.
    pub fn with_peer_discovery(mut self, provider: ProviderConfig) -> Self {
        self.manifest.peer_discovery = Some(provider);
        self
    }

    /// Adds a named provider adapter.
    pub fn with_adapter(
        mut self,
        name: impl Into<String>,
        provider: ProviderConfig,
    ) -> ContractResult<Self> {
        let name = name.into();
        validate_text("deployment.adapter.name", &name, 128)?;
        self.manifest.adapters.insert(name, provider);
        Ok(self)
    }

    /// Adds a non-sensitive environment literal.
    pub fn with_environment_literal(
        mut self,
        name: impl Into<String>,
        value: impl Into<String>,
    ) -> ContractResult<Self> {
        let name = name.into();
        let value = value.into();
        validate_environment_name(&name)?;
        if is_sensitive_key(&name) {
            return Err(ContractError::SecretValue {
                field: format!("environment.{name}"),
            });
        }
        validate_text("environment.value", &value, 8_192)?;
        self.manifest
            .environment
            .insert(name, EnvironmentBinding::Literal(value));
        Ok(self)
    }

    /// Adds an environment variable backed by a secret reference.
    pub fn with_environment_secret(
        mut self,
        name: impl Into<String>,
        secret: SecretRef,
    ) -> ContractResult<Self> {
        let name = name.into();
        validate_environment_name(&name)?;
        self.manifest
            .environment
            .insert(name, EnvironmentBinding::Secret(secret));
        Ok(self)
    }

    /// Builds and validates the deployment contract.
    pub fn build(self) -> ContractResult<DeploymentManifestV1> {
        self.manifest.validate()?;
        Ok(self.manifest)
    }
}

impl DeploymentManifestV1 {
    /// Starts a deployment manifest builder.
    pub fn builder(
        installation_id: InstallationId,
        application_id: ApplicationId,
        mode: RuntimeMode,
        storage: ProviderConfig,
        network: NetworkConfig,
    ) -> DeploymentManifestBuilder {
        DeploymentManifestBuilder::new(installation_id, application_id, mode, storage, network)
    }

    /// Returns the manifest schema version.
    pub fn manifest_version(&self) -> u16 {
        self.manifest_version
    }

    /// Returns the installation identity.
    pub fn installation_id(&self) -> &InstallationId {
        &self.installation_id
    }

    /// Returns the installed application identity.
    pub fn application_id(&self) -> &ApplicationId {
        &self.application_id
    }

    /// Returns the explicit runtime mode.
    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }

    /// Returns installation-owned supervisor settings.
    pub fn supervisor(&self) -> DeploymentSupervisorConfig {
        self.supervisor
    }

    /// Returns the optional control-plane provider.
    pub fn control_plane(&self) -> Option<&ProviderConfig> {
        self.control_plane.as_ref()
    }

    /// Returns the optional coordination-store provider.
    pub fn coordination_store(&self) -> Option<&ProviderConfig> {
        self.coordination_store.as_ref()
    }

    /// Returns the optional installation secret provider.
    pub fn secret_provider(&self) -> Option<&ProviderConfig> {
        self.secret_provider.as_ref()
    }

    /// Returns the optional durable job provider.
    pub fn job_provider(&self) -> Option<&ProviderConfig> {
        self.job_provider.as_ref()
    }

    /// Returns installation secret references.
    pub fn secrets(&self) -> &BTreeMap<String, SecretRef> {
        &self.secrets
    }

    /// Returns installation-owned paths.
    pub fn paths(&self) -> &BTreeMap<String, String> {
        &self.paths
    }

    /// Returns volume mounts.
    pub fn volumes(&self) -> &[VolumeMount] {
        &self.volumes
    }

    /// Returns the selected storage provider.
    pub fn storage(&self) -> &ProviderConfig {
        &self.storage
    }

    /// Returns the optional database provider.
    pub fn database(&self) -> Option<&ProviderConfig> {
        self.database.as_ref()
    }

    /// Returns the optional update provider.
    pub fn update_provider(&self) -> Option<&ProviderConfig> {
        self.update_provider.as_ref()
    }

    /// Returns installation network configuration.
    pub fn network(&self) -> &NetworkConfig {
        &self.network
    }

    /// Returns the optional peer-discovery provider.
    pub fn peer_discovery(&self) -> Option<&ProviderConfig> {
        self.peer_discovery.as_ref()
    }

    /// Returns named adapters.
    pub fn adapters(&self) -> &BTreeMap<String, ProviderConfig> {
        &self.adapters
    }

    /// Returns environment bindings.
    pub fn environment(&self) -> &BTreeMap<String, EnvironmentBinding> {
        &self.environment
    }

    /// Validates mode invariants and rejects embedded secrets.
    pub fn validate(&self) -> ContractResult<()> {
        if self.manifest_version != DEPLOYMENT_MANIFEST_VERSION {
            return Err(ContractError::InvalidValue {
                field: "manifest_version",
                reason: "unsupported deployment manifest version",
            });
        }
        match self.mode {
            RuntimeMode::Standalone => {
                if self.control_plane.is_some()
                    || self.coordination_store.is_some()
                    || self.peer_discovery.is_some()
                    || self.job_provider.is_some()
                {
                    return Err(ContractError::InvalidValue {
                        field: "mode",
                        reason: "standalone mode forbids distributed coordination providers",
                    });
                }
            }
            RuntimeMode::Cluster => {
                if self.control_plane.is_none() || self.peer_discovery.is_none() {
                    return Err(ContractError::InvalidValue {
                        field: "mode",
                        reason: "cluster mode requires control plane and peer discovery",
                    });
                }
            }
        }
        self.storage.validate()?;
        self.network.validate()?;
        self.supervisor.validate()?;
        for provider in self
            .control_plane
            .iter()
            .chain(self.coordination_store.iter())
            .chain(self.secret_provider.iter())
            .chain(self.job_provider.iter())
            .chain(self.database.iter())
            .chain(self.update_provider.iter())
            .chain(self.peer_discovery.iter())
            .chain(self.adapters.values())
        {
            provider.validate()?;
        }
        for (name, path) in &self.paths {
            validate_text("deployment.path.name", name, 128)?;
            validate_text("deployment.path", path, 2_048)?;
        }
        for volume in &self.volumes {
            volume.validate()?;
        }
        for name in self.secrets.keys() {
            validate_text("deployment.secret.name", name, 128)?;
        }
        for (name, binding) in &self.environment {
            validate_environment_name(name)?;
            match binding {
                EnvironmentBinding::Literal(value) => {
                    if is_sensitive_key(name) {
                        return Err(ContractError::SecretValue {
                            field: format!("environment.{name}"),
                        });
                    }
                    validate_text("environment.value", value, 8_192)?;
                }
                EnvironmentBinding::Secret(_) => {}
            }
        }
        Ok(())
    }
}

impl TryFrom<DeploymentManifestData> for DeploymentManifestV1 {
    type Error = ContractError;

    fn try_from(data: DeploymentManifestData) -> Result<Self, Self::Error> {
        let manifest = Self {
            manifest_version: data.manifest_version,
            installation_id: data.installation_id,
            application_id: data.application_id,
            mode: data.mode,
            supervisor: data.supervisor,
            control_plane: data.control_plane,
            coordination_store: data.coordination_store,
            secret_provider: data.secret_provider,
            job_provider: data.job_provider,
            secrets: data.secrets,
            paths: data.paths,
            volumes: data.volumes,
            storage: data.storage,
            database: data.database,
            update_provider: data.update_provider,
            network: data.network,
            peer_discovery: data.peer_discovery,
            adapters: data.adapters,
            environment: data.environment,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

pub(super) fn validate_setting(key: &str, value: &str) -> ContractResult<()> {
    validate_text("provider.setting.key", key, 128)?;
    validate_text("provider.setting.value", value, 8_192)?;
    if is_sensitive_key(key) {
        return Err(ContractError::SecretValue {
            field: format!("provider.settings.{key}"),
        });
    }
    Ok(())
}

fn validate_environment_name(name: &str) -> ContractResult<()> {
    validate_text("environment.name", name, 128)?;
    if !name.chars().all(|character| {
        character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_'
    }) {
        return Err(ContractError::InvalidValue {
            field: "environment.name",
            reason: "must contain only ASCII uppercase letters, digits and underscores",
        });
    }
    Ok(())
}
