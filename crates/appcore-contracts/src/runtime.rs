// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime-produced manifest contract.

use crate::identifiers::{is_sensitive_key, validate_text};
use crate::{
    BuildId, CapabilityId, ContractError, ContractResult, CoreId, CoreProfile, FeatureId, NodeId,
    ProviderId, RuntimeMode,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Schema version written by [`RuntimeManifestV1`].
pub const RUNTIME_MANIFEST_VERSION: u16 = 1;

/// Coarse health state produced by a runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeHealthStatus {
    /// Runtime is operating normally.
    Healthy,
    /// Runtime remains available with reduced guarantees.
    Degraded,
    /// Runtime is unable to serve its declared contract.
    Unhealthy,
}

/// Observable health snapshot without application data or secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeHealth {
    status: RuntimeHealthStatus,
    checked_at_ms: u64,
    details: BTreeMap<String, String>,
}

impl RuntimeHealth {
    /// Creates an empty runtime health snapshot.
    pub fn new(status: RuntimeHealthStatus, checked_at_ms: u64) -> Self {
        Self {
            status,
            checked_at_ms,
            details: BTreeMap::new(),
        }
    }

    /// Adds a non-sensitive diagnostic detail.
    pub fn with_detail(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> ContractResult<Self> {
        let key = key.into();
        let value = value.into();
        validate_health_detail(&key, &value)?;
        self.details.insert(key, value);
        Ok(self)
    }

    /// Returns the health status.
    pub fn status(&self) -> RuntimeHealthStatus {
        self.status
    }

    /// Returns the snapshot timestamp in Unix milliseconds.
    pub fn checked_at_ms(&self) -> u64 {
        self.checked_at_ms
    }

    /// Returns non-sensitive health details.
    pub fn details(&self) -> &BTreeMap<String, String> {
        &self.details
    }

    fn validate(&self) -> ContractResult<()> {
        for (key, value) in &self.details {
            validate_health_detail(key, value)?;
        }
        Ok(())
    }
}

/// Operational state relevant to routing and scheduling decisions.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeOperationalMode {
    /// Runtime is initializing local infrastructure.
    Starting,
    /// Runtime is discovering cluster peers.
    Discovering,
    /// Runtime is synchronizing state.
    Syncing,
    /// Runtime accepts read-only work.
    ReadOnly,
    /// Runtime accepts reads and writes.
    #[default]
    ReadWrite,
    /// Runtime remains partially available.
    Degraded,
    /// Runtime is detached from required cluster coordination.
    Isolated,
}

impl RuntimeOperationalMode {
    /// Reports whether local queries are allowed.
    pub fn allows_local_queries(self) -> bool {
        matches!(
            self,
            Self::ReadOnly | Self::ReadWrite | Self::Degraded | Self::Isolated
        )
    }

    /// Reports whether writes are allowed.
    pub fn allows_writes(self) -> bool {
        matches!(self, Self::ReadWrite)
    }

    /// Returns the stable serialized label.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Discovering => "discovering",
            Self::Syncing => "syncing",
            Self::ReadOnly => "read_only",
            Self::ReadWrite => "read_write",
            Self::Degraded => "degraded",
            Self::Isolated => "isolated",
        }
    }
}

impl TryFrom<&str> for RuntimeOperationalMode {
    type Error = ContractError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "starting" => Ok(Self::Starting),
            "discovering" => Ok(Self::Discovering),
            "syncing" => Ok(Self::Syncing),
            "read_only" => Ok(Self::ReadOnly),
            "read_write" => Ok(Self::ReadWrite),
            "readonly" | "readwrite" => Err(ContractError::InvalidValue {
                field: "operational_mode",
                reason: "NO MORE SUPPORTED PLEASE UPDATE",
            }),
            "degraded" => Ok(Self::Degraded),
            "isolated" => Ok(Self::Isolated),
            _ => Err(ContractError::InvalidValue {
                field: "operational_mode",
                reason: "unsupported operational mode",
            }),
        }
    }
}

/// Runtime-owned description of a running host.
///
/// Application identity and application metadata are deliberately absent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "RuntimeManifestData")]
pub struct RuntimeManifestV1 {
    manifest_version: u16,
    runtime_version: String,
    protocol_version: String,
    build_id: BuildId,
    features: BTreeSet<FeatureId>,
    node_id: NodeId,
    core_id: CoreId,
    mode: RuntimeMode,
    platform: String,
    architecture: String,
    storage_backend: ProviderId,
    health: RuntimeHealth,
    operational_mode: RuntimeOperationalMode,
    loaded_capabilities: BTreeSet<CapabilityId>,
    core_profile: CoreProfile,
}

#[derive(Deserialize)]
struct RuntimeManifestData {
    manifest_version: u16,
    runtime_version: String,
    protocol_version: String,
    build_id: BuildId,
    features: BTreeSet<FeatureId>,
    node_id: NodeId,
    core_id: CoreId,
    mode: RuntimeMode,
    platform: String,
    architecture: String,
    storage_backend: ProviderId,
    health: RuntimeHealth,
    operational_mode: RuntimeOperationalMode,
    loaded_capabilities: BTreeSet<CapabilityId>,
    core_profile: CoreProfile,
}

impl RuntimeManifestV1 {
    /// Creates a runtime-produced manifest from runtime and node facts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        runtime_version: impl Into<String>,
        protocol_version: impl Into<String>,
        build_id: BuildId,
        node_id: NodeId,
        core_id: CoreId,
        mode: RuntimeMode,
        platform: impl Into<String>,
        architecture: impl Into<String>,
        storage_backend: ProviderId,
        health: RuntimeHealth,
        core_profile: CoreProfile,
    ) -> ContractResult<Self> {
        let manifest = Self {
            manifest_version: RUNTIME_MANIFEST_VERSION,
            runtime_version: runtime_version.into(),
            protocol_version: protocol_version.into(),
            build_id,
            features: BTreeSet::new(),
            node_id,
            core_id,
            mode,
            platform: platform.into(),
            architecture: architecture.into(),
            storage_backend,
            health,
            operational_mode: RuntimeOperationalMode::Starting,
            loaded_capabilities: BTreeSet::new(),
            core_profile,
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Adds a runtime feature.
    pub fn with_feature(mut self, feature: FeatureId) -> Self {
        self.features.insert(feature);
        self
    }

    /// Adds a capability loaded by the current host.
    pub fn with_loaded_capability(mut self, capability: CapabilityId) -> ContractResult<Self> {
        if !self.core_profile.capabilities().contains(&capability) {
            return Err(ContractError::InvalidValue {
                field: "loaded_capabilities",
                reason: "capability is not declared by the core profile",
            });
        }
        self.loaded_capabilities.insert(capability);
        Ok(self)
    }

    /// Replaces observable health.
    pub fn with_health(mut self, health: RuntimeHealth) -> ContractResult<Self> {
        health.validate()?;
        self.health = health;
        Ok(self)
    }

    /// Replaces the current operational mode.
    pub fn with_operational_mode(mut self, mode: RuntimeOperationalMode) -> Self {
        self.operational_mode = mode;
        self
    }

    /// Returns the manifest schema version.
    pub fn manifest_version(&self) -> u16 {
        self.manifest_version
    }

    /// Returns the AppCore runtime version.
    pub fn runtime_version(&self) -> &str {
        &self.runtime_version
    }

    /// Returns the distributed protocol version.
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }

    /// Returns the immutable runtime build identity.
    pub fn build_id(&self) -> &BuildId {
        &self.build_id
    }

    /// Returns enabled runtime features.
    pub fn features(&self) -> &BTreeSet<FeatureId> {
        &self.features
    }

    /// Returns the host node identity.
    pub fn node_id(&self) -> &NodeId {
        &self.node_id
    }

    /// Returns the executable core identity.
    pub fn core_id(&self) -> &CoreId {
        &self.core_id
    }

    /// Returns the explicit standalone or cluster mode.
    pub fn mode(&self) -> RuntimeMode {
        self.mode
    }

    /// Returns the host platform.
    pub fn platform(&self) -> &str {
        &self.platform
    }

    /// Returns the host architecture.
    pub fn architecture(&self) -> &str {
        &self.architecture
    }

    /// Returns the active storage backend identity.
    pub fn storage_backend(&self) -> &ProviderId {
        &self.storage_backend
    }

    /// Returns observable runtime health.
    pub fn health(&self) -> &RuntimeHealth {
        &self.health
    }

    /// Returns the current operational mode.
    pub fn operational_mode(&self) -> RuntimeOperationalMode {
        self.operational_mode
    }

    /// Returns capabilities loaded by this runtime.
    pub fn loaded_capabilities(&self) -> &BTreeSet<CapabilityId> {
        &self.loaded_capabilities
    }

    /// Returns the scheduler-facing core profile.
    pub fn core_profile(&self) -> &CoreProfile {
        &self.core_profile
    }

    /// Validates runtime facts and cross-field capability declarations.
    pub fn validate(&self) -> ContractResult<()> {
        if self.manifest_version != RUNTIME_MANIFEST_VERSION {
            return Err(ContractError::InvalidValue {
                field: "manifest_version",
                reason: "unsupported runtime manifest version",
            });
        }
        validate_text("runtime_version", &self.runtime_version, 64)?;
        validate_text("protocol_version", &self.protocol_version, 64)?;
        validate_text("platform", &self.platform, 128)?;
        validate_text("architecture", &self.architecture, 128)?;
        self.health.validate()?;
        self.core_profile.validate()?;
        if !self
            .loaded_capabilities
            .is_subset(self.core_profile.capabilities())
        {
            return Err(ContractError::InvalidValue {
                field: "loaded_capabilities",
                reason: "loaded capabilities must be declared by the core profile",
            });
        }
        Ok(())
    }
}

impl TryFrom<RuntimeManifestData> for RuntimeManifestV1 {
    type Error = ContractError;

    fn try_from(data: RuntimeManifestData) -> Result<Self, Self::Error> {
        let manifest = Self {
            manifest_version: data.manifest_version,
            runtime_version: data.runtime_version,
            protocol_version: data.protocol_version,
            build_id: data.build_id,
            features: data.features,
            node_id: data.node_id,
            core_id: data.core_id,
            mode: data.mode,
            platform: data.platform,
            architecture: data.architecture,
            storage_backend: data.storage_backend,
            health: data.health,
            operational_mode: data.operational_mode,
            loaded_capabilities: data.loaded_capabilities,
            core_profile: data.core_profile,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

fn validate_health_detail(key: &str, value: &str) -> ContractResult<()> {
    validate_text("health.detail.key", key, 128)?;
    validate_text("health.detail.value", value, 2_048)?;
    if is_sensitive_key(key) {
        return Err(ContractError::SecretValue {
            field: format!("health.details.{key}"),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CoreRole, LeadershipMode, LeadershipRequirement, ResourceProfile, SchedulingProfile,
        ServiceId, WorkloadClass,
    };

    fn profile() -> CoreProfile {
        let service = ServiceId::new("document.extract").unwrap();
        CoreProfile::new(
            CoreRole::Compute,
            service.clone(),
            [CapabilityId::new("document.extract").unwrap()],
            LeadershipRequirement::new(service, LeadershipMode::Required, 30_000).unwrap(),
            ResourceProfile::new(Some(8), Some(16_000_000_000), 1),
            SchedulingProfile::new(10, 5, 4, WorkloadClass::Compute).unwrap(),
        )
        .unwrap()
    }

    fn manifest() -> RuntimeManifestV1 {
        RuntimeManifestV1::new(
            "0.6.1",
            "1",
            BuildId::new("build-123").unwrap(),
            NodeId::new("node-a").unwrap(),
            CoreId::new("core-a").unwrap(),
            RuntimeMode::Cluster,
            "linux",
            "aarch64",
            ProviderId::new("file").unwrap(),
            RuntimeHealth::new(RuntimeHealthStatus::Healthy, 100),
            profile(),
        )
        .unwrap()
        .with_loaded_capability(CapabilityId::new("document.extract").unwrap())
        .unwrap()
    }

    #[test]
    fn runtime_manifest_contains_runtime_facts_only() {
        let encoded = serde_json::to_value(manifest()).unwrap();
        assert!(encoded.get("application_id").is_none());
        assert!(encoded.get("vendor").is_none());
        assert_eq!(encoded["mode"], "cluster");
    }

    #[test]
    fn runtime_manifest_round_trip_revalidates_capabilities() {
        let manifest = manifest();
        let encoded = serde_json::to_string(&manifest).unwrap();
        let decoded: RuntimeManifestV1 = serde_json::from_str(&encoded).unwrap();
        assert_eq!(manifest, decoded);
    }

    #[test]
    fn runtime_manifest_matches_v1_fixture() {
        let expected: serde_json::Value =
            serde_json::from_str(include_str!("fixtures/runtime-manifest-v1.json")).unwrap();
        assert_eq!(serde_json::to_value(manifest()).unwrap(), expected);
        let decoded: RuntimeManifestV1 = serde_json::from_value(expected).unwrap();
        assert_eq!(decoded, manifest());
    }

    #[test]
    fn health_rejects_sensitive_details() {
        assert!(RuntimeHealth::new(RuntimeHealthStatus::Healthy, 0)
            .with_detail("access_token", "raw")
            .is_err());
    }
}
