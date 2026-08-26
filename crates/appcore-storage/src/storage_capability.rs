// =============================================================================
//        #######
//     ###       ###     F: storage_capability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Explicit bounded capability descriptors for storage-provider preflight.

use appcore_contracts::{ProviderConfig, ProviderId};
use std::collections::{BTreeMap, BTreeSet};

/// Version of the first storage capability descriptor contract.
pub const STORAGE_CAPABILITY_DESCRIPTOR_VERSION_V1: u16 = 1;
/// Deployment provider setting containing comma-separated required capabilities.
pub const STORAGE_REQUIRED_CAPABILITIES_SETTING: &str = "required_capabilities";
/// Exact number of capability kinds defined by the V1 descriptor.
pub const STORAGE_CAPABILITY_COUNT_V1: usize = 7;
/// Maximum provider descriptors admitted to one preflight catalog.
pub const MAX_STORAGE_CAPABILITY_PROVIDERS_V1: usize = 32;

/// One provider-independent storage guarantee in descriptor V1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StorageCapabilityV1 {
    /// Real atomic unit-of-work transactions.
    Transactions,
    /// Caller-visible locking with documented exclusion semantics.
    Locking,
    /// Provider-consistent snapshots.
    Snapshot,
    /// Bounded incremental reads and writes.
    Streaming,
    /// Consistent backup while the provider remains available.
    OnlineBackup,
    /// Concurrent access from independent local processes.
    MultiProcess,
    /// Concurrent access from independent hosts.
    MultiHost,
}

impl StorageCapabilityV1 {
    /// Returns the stable deployment spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transactions => "transactions",
            Self::Locking => "locking",
            Self::Snapshot => "snapshot",
            Self::Streaming => "streaming",
            Self::OnlineBackup => "online_backup",
            Self::MultiProcess => "multi_process",
            Self::MultiHost => "multi_host",
        }
    }

    fn parse(value: &str) -> Result<Self, StorageCapabilityError> {
        match value {
            "transactions" => Ok(Self::Transactions),
            "locking" => Ok(Self::Locking),
            "snapshot" => Ok(Self::Snapshot),
            "streaming" => Ok(Self::Streaming),
            "online_backup" => Ok(Self::OnlineBackup),
            "multi_process" => Ok(Self::MultiProcess),
            "multi_host" => Ok(Self::MultiHost),
            _ => Err(StorageCapabilityError::UnknownRequirement),
        }
    }
}

impl std::fmt::Display for StorageCapabilityV1 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Typed, redacted storage capability preflight failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageCapabilityError {
    /// A provider advertised an invalid stable identity.
    InvalidDescriptor,
    /// A requirement used an unknown or empty capability spelling.
    UnknownRequirement,
    /// A capability appeared more than once in the bounded requirement list.
    DuplicateRequirement(StorageCapabilityV1),
    /// More provider descriptors were registered than the fixed catalog bound.
    CatalogFull,
    /// A provider descriptor was registered twice.
    DuplicateProvider(ProviderId),
    /// No descriptor exists for the explicitly selected provider.
    ProviderUnavailable(ProviderId),
    /// The selected provider does not supply an exact required guarantee.
    MissingCapability {
        /// Explicitly selected provider identity.
        provider_id: ProviderId,
        /// Provider-independent guarantee that is absent.
        capability: StorageCapabilityV1,
    },
}

impl std::fmt::Display for StorageCapabilityError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDescriptor => {
                formatter.write_str("storage capability descriptor identity is invalid")
            }
            Self::UnknownRequirement => {
                formatter.write_str("storage capability requirement is unknown")
            }
            Self::DuplicateRequirement(capability) => write!(
                formatter,
                "storage capability requirement is duplicated: {capability}"
            ),
            Self::CatalogFull => formatter.write_str("storage capability provider catalog is full"),
            Self::DuplicateProvider(provider_id) => write!(
                formatter,
                "storage capability descriptor is duplicated for provider: {provider_id}"
            ),
            Self::ProviderUnavailable(provider_id) => write!(
                formatter,
                "storage capability descriptor is unavailable for provider: {provider_id}"
            ),
            Self::MissingCapability {
                provider_id,
                capability,
            } => write!(
                formatter,
                "storage provider {provider_id} does not support required capability {capability}"
            ),
        }
    }
}

impl std::error::Error for StorageCapabilityError {}

/// Immutable V1 descriptor advertised by one storage provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCapabilityDescriptorV1 {
    provider_id: ProviderId,
    capabilities: BTreeSet<StorageCapabilityV1>,
}

impl StorageCapabilityDescriptorV1 {
    /// Creates a bounded descriptor for one explicit provider identity.
    pub fn new(
        provider_id: ProviderId,
        capabilities: impl IntoIterator<Item = StorageCapabilityV1>,
    ) -> Self {
        Self {
            provider_id,
            capabilities: capabilities.into_iter().collect(),
        }
    }

    /// Returns the explicit descriptor version.
    pub const fn descriptor_version(&self) -> u16 {
        STORAGE_CAPABILITY_DESCRIPTOR_VERSION_V1
    }

    /// Returns the provider identity this descriptor binds.
    pub fn provider_id(&self) -> &ProviderId {
        &self.provider_id
    }

    /// Returns the bounded set of guarantees supplied by this provider.
    pub fn capabilities(&self) -> &BTreeSet<StorageCapabilityV1> {
        &self.capabilities
    }

    /// Reports whether the provider supplies an exact guarantee.
    pub fn supports(&self, capability: StorageCapabilityV1) -> bool {
        self.capabilities.contains(&capability)
    }

    /// Fails when any requested guarantee is absent.
    pub fn validate(
        &self,
        requirements: &StorageCapabilityRequirementsV1,
    ) -> Result<(), StorageCapabilityError> {
        for capability in requirements.capabilities() {
            if !self.supports(*capability) {
                return Err(StorageCapabilityError::MissingCapability {
                    provider_id: self.provider_id.clone(),
                    capability: *capability,
                });
            }
        }
        Ok(())
    }
}

/// Provider-independent bounded requirements resolved during manifest preflight.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StorageCapabilityRequirementsV1 {
    capabilities: BTreeSet<StorageCapabilityV1>,
}

impl StorageCapabilityRequirementsV1 {
    /// Creates no additional requirements.
    pub fn new() -> Self {
        Self::default()
    }

    /// Parses the opt-in deployment requirement setting without inference.
    pub fn from_provider_config(config: &ProviderConfig) -> Result<Self, StorageCapabilityError> {
        let Some(value) = config.settings().get(STORAGE_REQUIRED_CAPABILITIES_SETTING) else {
            return Ok(Self::new());
        };
        let mut requirements = Self::new();
        if value.trim().is_empty() {
            return Err(StorageCapabilityError::UnknownRequirement);
        }
        for raw in value.split(',') {
            let capability = StorageCapabilityV1::parse(raw.trim())?;
            requirements.require(capability)?;
        }
        Ok(requirements)
    }

    /// Adds one exact requirement and rejects duplicate declarations.
    pub fn require(
        &mut self,
        capability: StorageCapabilityV1,
    ) -> Result<(), StorageCapabilityError> {
        if !self.capabilities.insert(capability) {
            return Err(StorageCapabilityError::DuplicateRequirement(capability));
        }
        Ok(())
    }

    /// Includes one requirement derived from another validated contract field.
    pub fn include(&mut self, capability: StorageCapabilityV1) {
        self.capabilities.insert(capability);
    }

    /// Returns the bounded set of required guarantees.
    pub fn capabilities(&self) -> &BTreeSet<StorageCapabilityV1> {
        &self.capabilities
    }
}

/// Capability descriptor source implemented by a concrete storage provider.
pub trait StorageCapabilityProviderV1 {
    /// Returns an immutable descriptor without probing or opening the provider.
    fn storage_capabilities_v1(
        &self,
    ) -> Result<StorageCapabilityDescriptorV1, StorageCapabilityError>;
}

/// Bounded catalog used to resolve the descriptor for a selected provider.
#[derive(Debug, Clone, Default)]
pub struct StorageCapabilityCatalogV1 {
    descriptors: BTreeMap<ProviderId, StorageCapabilityDescriptorV1>,
}

impl StorageCapabilityCatalogV1 {
    /// Creates an empty bounded catalog.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one descriptor and rejects ambiguity or capacity overflow.
    pub fn register(
        &mut self,
        descriptor: StorageCapabilityDescriptorV1,
    ) -> Result<(), StorageCapabilityError> {
        if self.descriptors.contains_key(descriptor.provider_id()) {
            return Err(StorageCapabilityError::DuplicateProvider(
                descriptor.provider_id().clone(),
            ));
        }
        if self.descriptors.len() >= MAX_STORAGE_CAPABILITY_PROVIDERS_V1 {
            return Err(StorageCapabilityError::CatalogFull);
        }
        self.descriptors
            .insert(descriptor.provider_id().clone(), descriptor);
        Ok(())
    }

    /// Resolves and validates the explicitly selected provider without fallback.
    pub fn validate(
        &self,
        provider_id: &ProviderId,
        requirements: &StorageCapabilityRequirementsV1,
    ) -> Result<(), StorageCapabilityError> {
        self.descriptors
            .get(provider_id)
            .ok_or_else(|| StorageCapabilityError::ProviderUnavailable(provider_id.clone()))?
            .validate(requirements)
    }
}
