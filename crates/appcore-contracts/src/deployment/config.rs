// =============================================================================
//        #######
//     ###       ###     F: config.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;
use crate::deployment::manifest::validate_setting;

/// Schema version written by [`DeploymentManifestV1`].
pub const DEPLOYMENT_MANIFEST_VERSION: u16 = 1;

/// Installation-owned watchdog settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeploymentWatchdogConfig {
    enabled: bool,
    check_interval_ms: u64,
    stall_timeout_ms: u64,
}

impl DeploymentWatchdogConfig {
    /// Creates explicit watchdog settings.
    pub fn new(
        enabled: bool,
        check_interval_ms: u64,
        stall_timeout_ms: u64,
    ) -> ContractResult<Self> {
        let config = Self {
            enabled,
            check_interval_ms,
            stall_timeout_ms,
        };
        config.validate()?;
        Ok(config)
    }

    /// Reports whether watchdog health affects Runtime health.
    pub fn is_enabled(self) -> bool {
        self.enabled
    }

    /// Returns the independent watchdog evaluation interval.
    pub fn check_interval_ms(self) -> u64 {
        self.check_interval_ms
    }

    /// Returns the maximum interval without completed reconciliation.
    pub fn stall_timeout_ms(self) -> u64 {
        self.stall_timeout_ms
    }

    pub(super) fn validate(self) -> ContractResult<()> {
        if self.check_interval_ms == 0 || self.stall_timeout_ms == 0 {
            return Err(ContractError::InvalidValue {
                field: "supervisor.watchdog",
                reason: "watchdog intervals must be greater than zero",
            });
        }
        if self.enabled && self.stall_timeout_ms <= self.check_interval_ms {
            return Err(ContractError::InvalidValue {
                field: "supervisor.watchdog.stall_timeout_ms",
                reason: "must exceed check_interval_ms",
            });
        }
        Ok(())
    }
}

impl Default for DeploymentWatchdogConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval_ms: 1_000,
            stall_timeout_ms: 15_000,
        }
    }
}

/// Installation-owned supervisor settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DeploymentSupervisorConfig {
    watchdog: DeploymentWatchdogConfig,
}

impl DeploymentSupervisorConfig {
    /// Creates supervisor settings from watchdog policy.
    pub fn new(watchdog: DeploymentWatchdogConfig) -> Self {
        Self { watchdog }
    }

    /// Returns watchdog settings.
    pub fn watchdog(self) -> DeploymentWatchdogConfig {
        self.watchdog
    }

    pub(super) fn validate(self) -> ContractResult<()> {
        self.watchdog.validate()
    }
}

/// Reference to a secret held outside a manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SecretRef(String);

impl SecretRef {
    /// Creates a secret reference such as `env:APPCORE_TOKEN` or `vault:key-name`.
    pub fn new(reference: impl Into<String>) -> ContractResult<Self> {
        let reference = reference.into();
        validate_text("secret_ref", &reference, 512)?;
        let Some((scheme, target)) = reference.split_once(':') else {
            return Err(ContractError::InvalidValue {
                field: "secret_ref",
                reason: "a provider scheme is required",
            });
        };
        if !matches!(scheme, "env" | "file" | "vault" | "provider") || target.trim().is_empty() {
            return Err(ContractError::InvalidValue {
                field: "secret_ref",
                reason: "unsupported or empty secret reference",
            });
        }
        Ok(Self(reference))
    }

    /// Returns the non-secret reference string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for SecretRef {
    type Error = ContractError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<SecretRef> for String {
    fn from(value: SecretRef) -> Self {
        value.0
    }
}

/// Provider selection and its non-sensitive installation settings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    provider_id: ProviderId,
    endpoint: Option<String>,
    settings: BTreeMap<String, String>,
    secret_refs: BTreeMap<String, SecretRef>,
}

impl ProviderConfig {
    /// Selects a provider without implementation-specific settings.
    pub fn new(provider_id: ProviderId) -> Self {
        Self {
            provider_id,
            endpoint: None,
            settings: BTreeMap::new(),
            secret_refs: BTreeMap::new(),
        }
    }

    /// Adds an optional provider endpoint.
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> ContractResult<Self> {
        let endpoint = endpoint.into();
        validate_text("provider.endpoint", &endpoint, 2_048)?;
        self.endpoint = Some(endpoint);
        Ok(self)
    }

    /// Adds a non-sensitive provider setting.
    pub fn with_setting(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> ContractResult<Self> {
        let key = key.into();
        let value = value.into();
        validate_setting(&key, &value)?;
        self.settings.insert(key, value);
        Ok(self)
    }

    /// Adds a named reference to externally managed secret material.
    pub fn with_secret_ref(
        mut self,
        name: impl Into<String>,
        secret: SecretRef,
    ) -> ContractResult<Self> {
        let name = name.into();
        validate_text("provider.secret_ref.name", &name, 128)?;
        self.secret_refs.insert(name, secret);
        Ok(self)
    }

    /// Returns the selected provider identity.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    /// Returns the optional provider endpoint.
    pub fn endpoint(&self) -> Option<&str> {
        self.endpoint.as_deref()
    }

    /// Returns non-sensitive provider settings.
    pub fn settings(&self) -> &BTreeMap<String, String> {
        &self.settings
    }

    /// Returns provider secret references.
    pub fn secret_refs(&self) -> &BTreeMap<String, SecretRef> {
        &self.secret_refs
    }

    pub(super) fn validate(&self) -> ContractResult<()> {
        if let Some(endpoint) = &self.endpoint {
            validate_text("provider.endpoint", endpoint, 2_048)?;
        }
        for (key, value) in &self.settings {
            validate_setting(key, value)?;
        }
        for name in self.secret_refs.keys() {
            validate_text("provider.secret_ref.name", name, 128)?;
        }
        Ok(())
    }
}

/// TLS settings represented only by external certificate references.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TlsConfig {
    enabled: bool,
    certificate: Option<SecretRef>,
    private_key: Option<SecretRef>,
}

impl TlsConfig {
    /// Disables TLS at this adapter boundary.
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            certificate: None,
            private_key: None,
        }
    }

    /// Enables TLS using certificate and private-key references.
    pub fn enabled(certificate: SecretRef, private_key: SecretRef) -> Self {
        Self {
            enabled: true,
            certificate: Some(certificate),
            private_key: Some(private_key),
        }
    }

    /// Reports whether TLS is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Returns the certificate reference.
    pub fn certificate(&self) -> Option<&SecretRef> {
        self.certificate.as_ref()
    }

    /// Returns the private-key reference.
    pub fn private_key(&self) -> Option<&SecretRef> {
        self.private_key.as_ref()
    }

    pub(super) fn validate(&self) -> ContractResult<()> {
        if self.enabled && (self.certificate.is_none() || self.private_key.is_none()) {
            return Err(ContractError::InvalidValue {
                field: "network.tls",
                reason: "certificate and private-key references are required",
            });
        }
        if !self.enabled && (self.certificate.is_some() || self.private_key.is_some()) {
            return Err(ContractError::InvalidValue {
                field: "network.tls",
                reason: "disabled TLS must not retain key material references",
            });
        }
        Ok(())
    }
}

/// Network adapters selected for one installation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetworkConfig {
    listen_addresses: Vec<String>,
    peer_transport: ProviderId,
    command_transport: ProviderId,
    tls: TlsConfig,
}

impl NetworkConfig {
    /// Creates network configuration using explicit transport providers.
    pub fn new(peer_transport: ProviderId, command_transport: ProviderId) -> Self {
        Self {
            listen_addresses: Vec::new(),
            peer_transport,
            command_transport,
            tls: TlsConfig::disabled(),
        }
    }

    /// Adds a listen address owned by this installation.
    pub fn with_listen_address(mut self, address: impl Into<String>) -> ContractResult<Self> {
        let address = address.into();
        validate_text("network.listen_address", &address, 2_048)?;
        self.listen_addresses.push(address);
        Ok(self)
    }

    /// Replaces TLS settings.
    pub fn with_tls(mut self, tls: TlsConfig) -> ContractResult<Self> {
        tls.validate()?;
        self.tls = tls;
        Ok(self)
    }

    /// Returns listen addresses.
    pub fn listen_addresses(&self) -> &[String] {
        &self.listen_addresses
    }

    /// Returns the peer transport provider.
    pub fn peer_transport(&self) -> &ProviderId {
        &self.peer_transport
    }

    /// Returns the command transport provider.
    pub fn command_transport(&self) -> &ProviderId {
        &self.command_transport
    }

    /// Returns TLS settings.
    pub fn tls(&self) -> &TlsConfig {
        &self.tls
    }

    pub(super) fn validate(&self) -> ContractResult<()> {
        for address in &self.listen_addresses {
            validate_text("network.listen_address", address, 2_048)?;
        }
        self.tls.validate()
    }
}

/// Installation volume mounted for an application or runtime adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VolumeMount {
    name: String,
    source: String,
    target: String,
    read_only: bool,
}

impl VolumeMount {
    /// Creates a named volume mount.
    pub fn new(
        name: impl Into<String>,
        source: impl Into<String>,
        target: impl Into<String>,
        read_only: bool,
    ) -> ContractResult<Self> {
        let mount = Self {
            name: name.into(),
            source: source.into(),
            target: target.into(),
            read_only,
        };
        mount.validate()?;
        Ok(mount)
    }

    /// Returns the logical volume name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the installation-owned source.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the mount target exposed to the application.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Reports whether the mount is read-only.
    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    pub(super) fn validate(&self) -> ContractResult<()> {
        validate_text("volume.name", &self.name, 128)?;
        validate_text("volume.source", &self.source, 2_048)?;
        validate_text("volume.target", &self.target, 2_048)
    }
}

/// Environment variable value or external secret binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum EnvironmentBinding {
    /// Non-sensitive literal configuration value.
    Literal(String),
    /// Reference to externally managed secret material.
    Secret(SecretRef),
}
