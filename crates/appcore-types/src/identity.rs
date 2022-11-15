// =============================================================================
//        #######
//     ###       ###     F: identity.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime identity contract and compatibility checks between nodes.

use crate::error::{RuntimeError, RuntimeResult};
use crate::ids::{
    AppFamily, AppId, CapabilityName, ClusterId, CoreId, InstanceId, NodeId, ProtocolVersion,
    RuntimeContractVersion, SyncGroup, TenantId,
};

/// Compatibility result for two runtime identities.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CompatibilityStatus {
    /// All Runtime identity fields are compatible.
    Compatible,
    /// Application identities differ.
    DifferentAppId,
    /// Application compatibility families differ.
    DifferentAppFamily,
    /// Synchronization isolation groups differ.
    DifferentSyncGroup,
    /// Runtime contract versions differ.
    DifferentRuntimeContract,
}

/// Generic category for a Core. Custom values are accepted when they pass identifier validation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct CoreKind(String);

/// Compatibility result for distributed Core identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreCompatibilityStatus {
    /// All required distributed identity fields are compatible.
    Compatible,
    /// Tenant isolation boundaries differ.
    DifferentTenant,
    /// Required cluster boundaries differ.
    DifferentCluster,
    /// Distributed protocol versions are incompatible.
    IncompatibleProtocolVersion,
    /// Embedded Runtime identities are incompatible.
    IncompatibleRuntime(CompatibilityStatus),
    /// The peer does not advertise a required capability.
    MissingCapability(CapabilityName),
}

/// Compatibility policy for peer-to-peer or routed operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoreCompatibilityPolicy {
    /// Whether both Cores must belong to the same cluster.
    pub require_same_cluster: bool,
    /// Optional capability that the peer must advertise.
    pub required_capability: Option<CapabilityName>,
}

/// Runtime identity shared by nodes of the same application/runtime contract.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RuntimeIdentity {
    /// Stable application identity.
    pub app_id: AppId,
    /// Compatibility family shared by related application builds.
    pub app_family: AppFamily,
    /// Synchronization isolation group.
    pub sync_group: SyncGroup,
    /// Runtime contract implemented by the application.
    pub runtime_contract: RuntimeContractVersion,
    /// Identity of the Runtime node.
    pub node_id: NodeId,
}

/// Distributed identity for a running AppCore instance.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CoreIdentity {
    /// Tenant isolation boundary.
    pub tenant_id: TenantId,
    /// Cluster boundary within the tenant.
    pub cluster_id: ClusterId,
    /// Stable logical Core identity.
    pub core_id: CoreId,
    /// Unique running process identity.
    pub instance_id: InstanceId,
    /// Generic Core role.
    pub kind: CoreKind,
    /// Distributed protocol version.
    pub protocol_version: ProtocolVersion,
    /// Embedded Runtime/application identity.
    pub runtime: RuntimeIdentity,
}

impl CoreKind {
    /// General operational Core role.
    pub const OPERATIONAL: &'static str = "operational";
    /// Read-oriented Core role.
    pub const READ: &'static str = "read";
    /// Write-oriented Core role.
    pub const WRITE: &'static str = "write";
    /// Replica Core role.
    pub const REPLICA: &'static str = "replica";
    /// Background worker Core role.
    pub const WORKER: &'static str = "worker";
    /// Custom generic role declared by a consumer.
    pub const CUSTOM: &'static str = "custom";

    /// Creates a validated Core role.
    pub fn new(value: impl Into<String>) -> RuntimeResult<Self> {
        let value = value.into();
        crate::ids::validate_identifier("CoreKind", &value)?;
        Ok(Self(value))
    }

    /// Creates the default operational Core role.
    pub fn operational() -> Self {
        Self(Self::OPERATIONAL.to_string())
    }

    /// Returns the role identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Default for CoreKind {
    fn default() -> Self {
        Self::operational()
    }
}

impl TryFrom<&str> for CoreKind {
    type Error = RuntimeError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for CoreKind {
    type Error = RuntimeError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl Default for CoreCompatibilityPolicy {
    fn default() -> Self {
        Self {
            require_same_cluster: true,
            required_capability: None,
        }
    }
}

impl RuntimeIdentity {
    /// Checks compatibility excluding `node_id`, which may differ between nodes.
    pub fn check_compatibility(&self, other: &RuntimeIdentity) -> CompatibilityStatus {
        if self.app_id != other.app_id {
            return CompatibilityStatus::DifferentAppId;
        }

        if self.app_family != other.app_family {
            return CompatibilityStatus::DifferentAppFamily;
        }

        if self.sync_group != other.sync_group {
            return CompatibilityStatus::DifferentSyncGroup;
        }

        if self.runtime_contract != other.runtime_contract {
            return CompatibilityStatus::DifferentRuntimeContract;
        }

        CompatibilityStatus::Compatible
    }

    /// Returns `Ok(())` only for compatible identities.
    pub fn ensure_compatible(&self, other: &RuntimeIdentity) -> RuntimeResult<()> {
        let status = self.check_compatibility(other);
        if status == CompatibilityStatus::Compatible {
            return Ok(());
        }

        Err(RuntimeError::IncompatibleIdentity(status))
    }
}

impl CoreIdentity {
    /// Derives a standalone-compatible distributed identity from Runtime fields.
    pub fn from_runtime_defaults(runtime: RuntimeIdentity) -> RuntimeResult<Self> {
        Ok(Self {
            tenant_id: TenantId::new(runtime.app_id.as_str())?,
            cluster_id: ClusterId::new(runtime.sync_group.as_str())?,
            core_id: CoreId::new(runtime.node_id.as_str())?,
            instance_id: InstanceId::new(runtime.node_id.as_str())?,
            kind: CoreKind::default(),
            protocol_version: ProtocolVersion::default(),
            runtime,
        })
    }

    /// Evaluates distributed compatibility against an explicit policy.
    pub fn check_compatibility(
        &self,
        other: &CoreIdentity,
        policy: &CoreCompatibilityPolicy,
        other_capabilities: &[CapabilityName],
    ) -> CoreCompatibilityStatus {
        if self.tenant_id != other.tenant_id {
            return CoreCompatibilityStatus::DifferentTenant;
        }

        if policy.require_same_cluster && self.cluster_id != other.cluster_id {
            return CoreCompatibilityStatus::DifferentCluster;
        }

        if !self
            .protocol_version
            .is_compatible_with(other.protocol_version)
        {
            return CoreCompatibilityStatus::IncompatibleProtocolVersion;
        }

        let runtime_status = self.runtime.check_compatibility(&other.runtime);
        if runtime_status != CompatibilityStatus::Compatible {
            return CoreCompatibilityStatus::IncompatibleRuntime(runtime_status);
        }

        if let Some(required) = &policy.required_capability {
            if !other_capabilities
                .iter()
                .any(|capability| capability == required)
            {
                return CoreCompatibilityStatus::MissingCapability(required.clone());
            }
        }

        CoreCompatibilityStatus::Compatible
    }

    /// Returns success only when distributed compatibility requirements pass.
    pub fn ensure_compatible(
        &self,
        other: &CoreIdentity,
        policy: &CoreCompatibilityPolicy,
        other_capabilities: &[CapabilityName],
    ) -> RuntimeResult<()> {
        match self.check_compatibility(other, policy, other_capabilities) {
            CoreCompatibilityStatus::Compatible => Ok(()),
            status => Err(RuntimeError::IncompatibleCoreIdentity(status)),
        }
    }
}
