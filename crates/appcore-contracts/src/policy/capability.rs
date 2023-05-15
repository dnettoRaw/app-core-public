// =============================================================================
//        #######
//     ###       ###     F: capability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 10:59:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Ownership boundary for a capability declaration.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityClass {
    /// Infrastructure behavior owned and consumed by the Runtime.
    Infrastructure,
    /// Application behavior implemented by consumer code.
    #[default]
    Functional,
}

impl CapabilityClass {
    fn is_functional(&self) -> bool {
        *self == Self::Functional
    }
}

/// Where a capability may be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityVisibility {
    /// Only inside the current process.
    Local,
    /// Across compatible peers in the cluster.
    Cluster,
    /// Across peers belonging to the same tenant boundary.
    Tenant,
}

/// Application declaration for one generic capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDeclaration {
    id: CapabilityId,
    #[serde(default, skip_serializing_if = "CapabilityClass::is_functional")]
    class: CapabilityClass,
    version: String,
    mode: CapabilityMode,
    visibility: CapabilityVisibility,
    requires_leader: bool,
    idempotency_required: bool,
}

impl CapabilityDeclaration {
    /// Creates a capability declaration.
    pub fn new(
        id: CapabilityId,
        version: impl Into<String>,
        mode: CapabilityMode,
        visibility: CapabilityVisibility,
    ) -> ContractResult<Self> {
        let declaration = Self {
            id,
            class: CapabilityClass::Functional,
            version: version.into(),
            mode,
            visibility,
            requires_leader: false,
            idempotency_required: false,
        };
        declaration.validate()?;
        Ok(declaration)
    }

    /// Classifies the capability as Runtime infrastructure or application behavior.
    pub fn with_class(mut self, class: CapabilityClass) -> Self {
        self.class = class;
        self
    }

    /// Marks whether execution requires service leadership.
    pub fn with_leadership(mut self, required: bool) -> Self {
        self.requires_leader = required;
        self
    }

    /// Marks whether command invocations require an idempotency key.
    pub fn with_idempotency(mut self, required: bool) -> Self {
        self.idempotency_required = required;
        self
    }

    /// Returns the capability identity.
    pub fn id(&self) -> &CapabilityId {
        &self.id
    }

    /// Returns the capability ownership boundary.
    pub fn class(&self) -> CapabilityClass {
        self.class
    }

    /// Returns the declared capability version.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Returns the invocation mode.
    pub fn mode(&self) -> CapabilityMode {
        self.mode
    }

    /// Returns the visibility boundary.
    pub fn visibility(&self) -> CapabilityVisibility {
        self.visibility
    }

    /// Reports whether the capability requires leadership.
    pub fn requires_leader(&self) -> bool {
        self.requires_leader
    }

    /// Reports whether commands require an idempotency key.
    pub fn idempotency_required(&self) -> bool {
        self.idempotency_required
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        validate_text("capability.version", &self.version, 64)
    }
}

/// Runtime and protocol versions required by an application.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeRequirements {
    minimum_runtime_version: String,
    maximum_runtime_version: Option<String>,
    protocol_version: String,
    required_features: BTreeSet<FeatureId>,
}

impl RuntimeRequirements {
    /// Creates minimum runtime and protocol requirements.
    pub fn new(
        minimum_runtime_version: impl Into<String>,
        protocol_version: impl Into<String>,
    ) -> ContractResult<Self> {
        let requirements = Self {
            minimum_runtime_version: minimum_runtime_version.into(),
            maximum_runtime_version: None,
            protocol_version: protocol_version.into(),
            required_features: BTreeSet::new(),
        };
        requirements.validate()?;
        Ok(requirements)
    }

    /// Adds an inclusive maximum runtime version.
    pub fn with_maximum_runtime_version(
        mut self,
        version: impl Into<String>,
    ) -> ContractResult<Self> {
        let version = version.into();
        validate_text("runtime.maximum_version", &version, 64)?;
        self.maximum_runtime_version = Some(version);
        Ok(self)
    }

    /// Adds a runtime feature required by the application.
    pub fn with_required_feature(mut self, feature: FeatureId) -> Self {
        self.required_features.insert(feature);
        self
    }

    /// Returns the minimum compatible runtime version.
    pub fn minimum_runtime_version(&self) -> &str {
        &self.minimum_runtime_version
    }

    /// Returns the optional maximum compatible runtime version.
    pub fn maximum_runtime_version(&self) -> Option<&str> {
        self.maximum_runtime_version.as_deref()
    }

    /// Returns the required distributed protocol version.
    pub fn protocol_version(&self) -> &str {
        &self.protocol_version
    }

    /// Returns required runtime features.
    pub fn required_features(&self) -> &BTreeSet<FeatureId> {
        &self.required_features
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        validate_text("runtime.minimum_version", &self.minimum_runtime_version, 64)?;
        validate_text("runtime.protocol_version", &self.protocol_version, 64)?;
        if let Some(version) = &self.maximum_runtime_version {
            validate_text("runtime.maximum_version", version, 64)?;
        }
        Ok(())
    }
}
