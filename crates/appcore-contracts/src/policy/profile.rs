// =============================================================================
//        #######
//     ###       ###     F: profile.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

/// Generic role assigned to one executable core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoreRole {
    /// General-purpose application host.
    GeneralPurpose,
    /// Coordination and control workload.
    Control,
    /// Background worker workload.
    Worker,
    /// Storage-oriented workload.
    Storage,
    /// Compute-intensive workload.
    Compute,
    /// Extensible role unknown to the base runtime.
    Custom(String),
}

/// Resource availability or requirement of one core.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceProfile {
    cpu_cores: Option<u16>,
    memory_bytes: Option<u64>,
    gpu_count: u16,
}

impl ResourceProfile {
    /// Creates a resource profile.
    pub fn new(cpu_cores: Option<u16>, memory_bytes: Option<u64>, gpu_count: u16) -> Self {
        Self {
            cpu_cores,
            memory_bytes,
            gpu_count,
        }
    }

    /// Returns known CPU core count.
    pub fn cpu_cores(&self) -> Option<u16> {
        self.cpu_cores
    }

    /// Returns known memory in bytes.
    pub fn memory_bytes(&self) -> Option<u64> {
        self.memory_bytes
    }

    /// Returns available GPU count.
    pub fn gpu_count(&self) -> u16 {
        self.gpu_count
    }

    fn validate(&self) -> ContractResult<()> {
        if self.cpu_cores == Some(0) || self.memory_bytes == Some(0) {
            return Err(ContractError::InvalidValue {
                field: "resources",
                reason: "known CPU and memory values must be greater than zero",
            });
        }
        Ok(())
    }
}

/// Broad workload category used as one scheduler signal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkloadClass {
    /// Mixed or unspecified workload.
    #[default]
    General,
    /// Latency-sensitive interactive work.
    Interactive,
    /// Background batch work.
    Batch,
    /// Compute-intensive work.
    Compute,
    /// I/O-intensive work.
    Io,
}

/// Scheduler inputs advertised by one core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SchedulingProfile {
    weight: u16,
    priority: i16,
    max_concurrency: u32,
    available: bool,
    workload: WorkloadClass,
    affinity: BTreeSet<String>,
}

impl SchedulingProfile {
    /// Creates scheduler inputs with an empty affinity set.
    pub fn new(
        weight: u16,
        priority: i16,
        max_concurrency: u32,
        workload: WorkloadClass,
    ) -> ContractResult<Self> {
        if weight == 0 || max_concurrency == 0 {
            return Err(ContractError::InvalidValue {
                field: "scheduling",
                reason: "weight and max concurrency must be greater than zero",
            });
        }
        Ok(Self {
            weight,
            priority,
            max_concurrency,
            available: true,
            workload,
            affinity: BTreeSet::new(),
        })
    }

    /// Marks current scheduling availability.
    pub fn with_availability(mut self, available: bool) -> Self {
        self.available = available;
        self
    }

    /// Adds a validated affinity label.
    pub fn with_affinity(mut self, affinity: impl Into<String>) -> ContractResult<Self> {
        let affinity = affinity.into();
        validate_text("scheduling.affinity", &affinity, 128)?;
        self.affinity.insert(affinity);
        Ok(self)
    }

    /// Returns relative scheduler weight.
    pub fn weight(&self) -> u16 {
        self.weight
    }

    /// Returns scheduler priority.
    pub fn priority(&self) -> i16 {
        self.priority
    }

    /// Returns maximum accepted concurrency.
    pub fn max_concurrency(&self) -> u32 {
        self.max_concurrency
    }

    /// Reports whether this core currently accepts work.
    pub fn is_available(&self) -> bool {
        self.available
    }

    /// Returns the workload class.
    pub fn workload(&self) -> WorkloadClass {
        self.workload
    }

    /// Returns scheduler affinity labels.
    pub fn affinity(&self) -> &BTreeSet<String> {
        &self.affinity
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        if self.weight == 0 || self.max_concurrency == 0 {
            return Err(ContractError::InvalidValue {
                field: "scheduling",
                reason: "weight and max concurrency must be greater than zero",
            });
        }
        for affinity in &self.affinity {
            validate_text("scheduling.affinity", affinity, 128)?;
        }
        Ok(())
    }
}

/// Provider-independent profile advertised by one executable core.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreProfile {
    role: CoreRole,
    service_id: ServiceId,
    capabilities: BTreeSet<CapabilityId>,
    leadership: LeadershipRequirement,
    resources: ResourceProfile,
    scheduling: SchedulingProfile,
}

impl CoreProfile {
    /// Creates a profile whose leadership lease is scoped to the same service.
    pub fn new(
        role: CoreRole,
        service_id: ServiceId,
        capabilities: impl IntoIterator<Item = CapabilityId>,
        leadership: LeadershipRequirement,
        resources: ResourceProfile,
        scheduling: SchedulingProfile,
    ) -> ContractResult<Self> {
        if leadership.service_id() != &service_id {
            return Err(ContractError::InvalidValue {
                field: "core_profile.leadership",
                reason: "leadership must be scoped to the profile service",
            });
        }
        let profile = Self {
            role,
            service_id,
            capabilities: capabilities.into_iter().collect(),
            leadership,
            resources,
            scheduling,
        };
        profile.validate()?;
        Ok(profile)
    }

    /// Returns the generic core role.
    pub fn role(&self) -> &CoreRole {
        &self.role
    }

    /// Returns the service coordinated by this profile.
    pub fn service_id(&self) -> &ServiceId {
        &self.service_id
    }

    /// Returns advertised capabilities.
    pub fn capabilities(&self) -> &BTreeSet<CapabilityId> {
        &self.capabilities
    }

    /// Returns service-scoped leadership requirements.
    pub fn leadership(&self) -> &LeadershipRequirement {
        &self.leadership
    }

    /// Returns the resource profile.
    pub fn resources(&self) -> &ResourceProfile {
        &self.resources
    }

    /// Returns scheduler inputs.
    pub fn scheduling(&self) -> &SchedulingProfile {
        &self.scheduling
    }

    pub(crate) fn validate(&self) -> ContractResult<()> {
        if let CoreRole::Custom(role) = &self.role {
            validate_text("core_profile.role", role, 128)?;
        }
        self.leadership.validate()?;
        self.resources.validate()?;
        self.scheduling.validate()?;
        if self.leadership.service_id() != &self.service_id {
            return Err(ContractError::InvalidValue {
                field: "core_profile.leadership",
                reason: "leadership must be scoped to the profile service",
            });
        }
        Ok(())
    }
}
