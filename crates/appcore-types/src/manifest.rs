// =============================================================================
//        #######
//     ###       ###     F: manifest.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Distributed peer declaration derived from versioned application contracts.

use crate::error::RuntimeResult;
use crate::identity::{CoreCompatibilityPolicy, CoreCompatibilityStatus, CoreIdentity};
use crate::ids::CapabilityName;
use std::collections::BTreeMap;

/// Internal structured manifest used to assemble distributed peer state.
///
/// Public wire APIs expose the versioned `PeerAdvertisementV1` contract.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DistributedCoreManifest {
    /// Distributed Core identity.
    pub identity: CoreIdentity,
    /// Human-readable application name.
    pub app_name: String,
    /// Application version.
    pub app_version: String,
    /// Minimum compatible Runtime version.
    pub runtime_min_version: String,
    /// Optional maximum compatible Runtime version.
    pub runtime_max_version: Option<String>,
    /// Generic capabilities exposed by the Core.
    pub capabilities: Vec<CapabilityDescriptor>,
    /// Public endpoints without credentials.
    pub endpoints: Vec<PeerEndpoint>,
    /// Non-sensitive routing metadata.
    pub metadata: BTreeMap<String, String>,
}

/// Generic capability advertised by a distributed Core.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CapabilityDescriptor {
    /// Stable capability name.
    pub name: CapabilityName,
    /// Capability contract version.
    pub version: String,
    /// Invocation mode.
    pub mode: CapabilityMode,
    /// Routing visibility.
    pub visibility: CapabilityVisibility,
    /// Runtime requirements attached to the capability.
    pub requirements: CapabilityRequirements,
}

/// Invocation mode for a generic capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityMode {
    /// Side-effect-free read.
    Query,
    /// Mutating or important action.
    Command,
    /// Stream of values or events.
    Stream,
}

/// Network scope in which a capability may be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityVisibility {
    /// Capability is available only in the local process.
    Local,
    /// Capability is available inside the current cluster.
    Cluster,
    /// Capability may be resolved across clusters in the same tenant.
    Tenant,
}

/// Generic execution requirements used by routing policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct CapabilityRequirements {
    /// Whether service-scoped leadership is required.
    pub requires_leader: bool,
    /// Whether the capability performs no writes.
    pub read_only: bool,
    /// Whether mutating requests require an idempotency key.
    pub idempotency_required: bool,
}

/// Network endpoint a peer can advertise. Secrets and bearer tokens do not belong here.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PeerEndpoint {
    /// Logical endpoint name.
    pub name: String,
    /// Public endpoint URL.
    pub url: String,
    /// Transport protocol identifier.
    pub protocol: String,
    /// Non-sensitive transport metadata.
    pub metadata: BTreeMap<String, String>,
}

impl DistributedCoreManifest {
    /// Derives distributed peer state from the application contract and Core identity.
    pub fn from_application_manifest(
        manifest: &appcore_contracts::ApplicationManifestV1,
        identity: CoreIdentity,
    ) -> RuntimeResult<Self> {
        Ok(Self {
            identity,
            app_name: manifest.display_name().to_string(),
            app_version: manifest.application_version().to_string(),
            runtime_min_version: manifest
                .runtime_requirements()
                .minimum_runtime_version()
                .to_string(),
            runtime_max_version: manifest
                .runtime_requirements()
                .maximum_runtime_version()
                .map(ToOwned::to_owned),
            capabilities: manifest
                .capabilities()
                .iter()
                .map(CapabilityDescriptor::from_application_declaration)
                .collect::<RuntimeResult<Vec<_>>>()?,
            endpoints: Vec::new(),
            metadata: BTreeMap::new(),
        })
    }

    /// Returns the distributed identity.
    pub fn identity(&self) -> &CoreIdentity {
        &self.identity
    }

    /// Reports whether the Core advertises `capability`.
    pub fn supports_capability(&self, capability: &CapabilityName) -> bool {
        self.capabilities
            .iter()
            .any(|descriptor| &descriptor.name == capability)
    }

    /// Returns all advertised capability names.
    pub fn capability_names(&self) -> Vec<CapabilityName> {
        self.capabilities
            .iter()
            .map(|descriptor| descriptor.name.clone())
            .collect()
    }

    /// Evaluates this Core against a peer and compatibility policy.
    pub fn check_peer_compatibility(
        &self,
        peer: &DistributedCoreManifest,
        policy: &CoreCompatibilityPolicy,
    ) -> CoreCompatibilityStatus {
        self.identity.check_compatibility(
            &peer.identity,
            policy,
            peer.capability_names().as_slice(),
        )
    }
}

impl CapabilityDescriptor {
    /// Creates a capability descriptor without additional requirements.
    pub fn new(
        name: CapabilityName,
        version: impl Into<String>,
        mode: CapabilityMode,
        visibility: CapabilityVisibility,
    ) -> Self {
        Self {
            name,
            version: version.into(),
            mode,
            visibility,
            requirements: CapabilityRequirements::default(),
        }
    }

    /// Replaces execution requirements.
    pub fn with_requirements(mut self, requirements: CapabilityRequirements) -> Self {
        self.requirements = requirements;
        self
    }

    fn from_application_declaration(
        declaration: &appcore_contracts::CapabilityDeclaration,
    ) -> RuntimeResult<Self> {
        let mode = match declaration.mode() {
            appcore_contracts::CapabilityMode::Query => CapabilityMode::Query,
            appcore_contracts::CapabilityMode::Command => CapabilityMode::Command,
            appcore_contracts::CapabilityMode::Stream => CapabilityMode::Stream,
        };
        let visibility = match declaration.visibility() {
            appcore_contracts::CapabilityVisibility::Local => CapabilityVisibility::Local,
            appcore_contracts::CapabilityVisibility::Cluster => CapabilityVisibility::Cluster,
            appcore_contracts::CapabilityVisibility::Tenant => CapabilityVisibility::Tenant,
        };
        Ok(Self::new(
            CapabilityName::new(declaration.id().as_str())?,
            declaration.version(),
            mode,
            visibility,
        )
        .with_requirements(CapabilityRequirements {
            requires_leader: declaration.requires_leader(),
            read_only: declaration.mode() == appcore_contracts::CapabilityMode::Query,
            idempotency_required: declaration.idempotency_required(),
        }))
    }
}
