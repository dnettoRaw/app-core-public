// =============================================================================
//        #######
//     ###       ###     F: application.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Application-owned manifest contract.

use crate::identifiers::{is_sensitive_key, looks_like_local_path, looks_like_url, validate_text};
use crate::{
    ApplicationDependency, ApplicationId, CapabilityClass, CapabilityDeclaration, ContractError,
    ContractResult, FeatureId, HealthRequirements, JobPolicy, LeadershipMode,
    LeadershipRequirement, ModuleDeclaration, RuntimeRequirements, SchedulerRequirements,
    ServiceId, StorageDurability, StorageRequirements, UpdatePolicy,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Schema version written by [`ApplicationManifestV1`].
pub const APPLICATION_MANIFEST_VERSION: u16 = 1;

/// Contract published by an application and consumed by any compatible runtime.
///
/// It contains application intent and requirements only. Provider selection,
/// installation paths and secrets belong to the deployment manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "ApplicationManifestData")]
pub struct ApplicationManifestV1 {
    manifest_version: u16,
    application_id: ApplicationId,
    application_version: String,
    display_name: String,
    vendor: String,
    service_id: ServiceId,
    runtime: RuntimeRequirements,
    capabilities: Vec<CapabilityDeclaration>,
    leadership: Vec<LeadershipRequirement>,
    jobs: JobPolicy,
    dependencies: Vec<ApplicationDependency>,
    storage: StorageRequirements,
    scheduler: SchedulerRequirements,
    health: HealthRequirements,
    update: UpdatePolicy,
    modules: Vec<ModuleDeclaration>,
    feature_flags: BTreeMap<FeatureId, bool>,
    metadata: BTreeMap<String, String>,
}

#[derive(Deserialize)]
struct ApplicationManifestData {
    manifest_version: u16,
    application_id: ApplicationId,
    application_version: String,
    display_name: String,
    vendor: String,
    service_id: ServiceId,
    runtime: RuntimeRequirements,
    capabilities: Vec<CapabilityDeclaration>,
    leadership: Vec<LeadershipRequirement>,
    jobs: JobPolicy,
    dependencies: Vec<ApplicationDependency>,
    storage: StorageRequirements,
    scheduler: SchedulerRequirements,
    health: HealthRequirements,
    update: UpdatePolicy,
    modules: Vec<ModuleDeclaration>,
    feature_flags: BTreeMap<FeatureId, bool>,
    metadata: BTreeMap<String, String>,
}

impl ApplicationManifestV1 {
    /// Creates a minimal application manifest with conservative local defaults.
    pub fn new(
        application_id: ApplicationId,
        application_version: impl Into<String>,
        display_name: impl Into<String>,
        vendor: impl Into<String>,
        service_id: ServiceId,
        runtime: RuntimeRequirements,
    ) -> ContractResult<Self> {
        let manifest = Self {
            manifest_version: APPLICATION_MANIFEST_VERSION,
            application_id,
            application_version: application_version.into(),
            display_name: display_name.into(),
            vendor: vendor.into(),
            service_id,
            runtime,
            capabilities: Vec::new(),
            leadership: Vec::new(),
            jobs: JobPolicy::disabled(),
            dependencies: Vec::new(),
            storage: StorageRequirements::new(StorageDurability::Local, 0, false),
            scheduler: SchedulerRequirements::new(false, 0)?,
            health: HealthRequirements::new(30_000, 10_000, 3)?,
            update: UpdatePolicy::new("stable", false)?,
            modules: Vec::new(),
            feature_flags: BTreeMap::new(),
            metadata: BTreeMap::new(),
        };
        manifest.validate()?;
        Ok(manifest)
    }

    /// Adds a capability declaration.
    pub fn with_capability(mut self, capability: CapabilityDeclaration) -> ContractResult<Self> {
        if self
            .capabilities
            .iter()
            .any(|existing| existing.id() == capability.id())
        {
            return Err(ContractError::Duplicate {
                field: "capability",
                value: capability.id().to_string(),
            });
        }
        self.capabilities.push(capability);
        self.validate()?;
        Ok(self)
    }

    /// Adds a service-scoped leadership requirement.
    pub fn with_leadership(mut self, requirement: LeadershipRequirement) -> ContractResult<Self> {
        if self
            .leadership
            .iter()
            .any(|existing| existing.service_id() == requirement.service_id())
        {
            return Err(ContractError::Duplicate {
                field: "leadership.service_id",
                value: requirement.service_id().to_string(),
            });
        }
        self.leadership.push(requirement);
        self.validate()?;
        Ok(self)
    }

    /// Replaces the job policy.
    pub fn with_job_policy(mut self, policy: JobPolicy) -> ContractResult<Self> {
        self.jobs = policy;
        self.validate()?;
        Ok(self)
    }

    /// Adds an application dependency.
    pub fn with_dependency(mut self, dependency: ApplicationDependency) -> ContractResult<Self> {
        if self
            .dependencies
            .iter()
            .any(|existing| existing.application_id() == dependency.application_id())
        {
            return Err(ContractError::Duplicate {
                field: "dependency.application_id",
                value: dependency.application_id().to_string(),
            });
        }
        self.dependencies.push(dependency);
        self.validate()?;
        Ok(self)
    }

    /// Replaces provider-independent storage requirements.
    pub fn with_storage_requirements(mut self, requirements: StorageRequirements) -> Self {
        self.storage = requirements;
        self
    }

    /// Replaces scheduler requirements.
    pub fn with_scheduler_requirements(
        mut self,
        requirements: SchedulerRequirements,
    ) -> ContractResult<Self> {
        self.scheduler = requirements;
        self.validate()?;
        Ok(self)
    }

    /// Replaces health requirements.
    pub fn with_health_requirements(
        mut self,
        requirements: HealthRequirements,
    ) -> ContractResult<Self> {
        self.health = requirements;
        self.validate()?;
        Ok(self)
    }

    /// Replaces application update preferences.
    pub fn with_update_policy(mut self, policy: UpdatePolicy) -> ContractResult<Self> {
        self.update = policy;
        self.validate()?;
        Ok(self)
    }

    /// Adds an application module.
    pub fn with_module(mut self, module: ModuleDeclaration) -> ContractResult<Self> {
        if self
            .modules
            .iter()
            .any(|existing| existing.id() == module.id())
        {
            return Err(ContractError::Duplicate {
                field: "module",
                value: module.id().to_string(),
            });
        }
        self.modules.push(module);
        self.validate()?;
        Ok(self)
    }

    /// Sets one application feature flag.
    pub fn with_feature_flag(mut self, feature: FeatureId, enabled: bool) -> Self {
        self.feature_flags.insert(feature, enabled);
        self
    }

    /// Adds non-sensitive, portable application metadata.
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> ContractResult<Self> {
        let key = key.into();
        let value = value.into();
        validate_application_metadata(&key, &value)?;
        self.metadata.insert(key, value);
        Ok(self)
    }

    /// Returns the manifest schema version.
    pub fn manifest_version(&self) -> u16 {
        self.manifest_version
    }

    /// Returns the stable application identity.
    pub fn application_id(&self) -> &ApplicationId {
        &self.application_id
    }

    /// Returns the application version.
    pub fn application_version(&self) -> &str {
        &self.application_version
    }

    /// Returns the human-readable application name.
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns the application vendor.
    pub fn vendor(&self) -> &str {
        &self.vendor
    }

    /// Returns the primary service identity.
    pub fn service_id(&self) -> &ServiceId {
        &self.service_id
    }

    /// Returns runtime compatibility requirements.
    pub fn runtime_requirements(&self) -> &RuntimeRequirements {
        &self.runtime
    }

    /// Returns declared capabilities.
    pub fn capabilities(&self) -> &[CapabilityDeclaration] {
        &self.capabilities
    }

    /// Returns service-scoped leadership requirements.
    pub fn leadership(&self) -> &[LeadershipRequirement] {
        &self.leadership
    }

    /// Returns the job policy.
    pub fn job_policy(&self) -> &JobPolicy {
        &self.jobs
    }

    /// Returns application dependencies.
    pub fn dependencies(&self) -> &[ApplicationDependency] {
        &self.dependencies
    }

    /// Returns storage requirements.
    pub fn storage_requirements(&self) -> &StorageRequirements {
        &self.storage
    }

    /// Returns scheduler requirements.
    pub fn scheduler_requirements(&self) -> &SchedulerRequirements {
        &self.scheduler
    }

    /// Returns health requirements.
    pub fn health_requirements(&self) -> &HealthRequirements {
        &self.health
    }

    /// Returns update preferences.
    pub fn update_policy(&self) -> &UpdatePolicy {
        &self.update
    }

    /// Returns application modules.
    pub fn modules(&self) -> &[ModuleDeclaration] {
        &self.modules
    }

    /// Returns application feature flags.
    pub fn feature_flags(&self) -> &BTreeMap<FeatureId, bool> {
        &self.feature_flags
    }

    /// Returns non-sensitive portable metadata.
    pub fn metadata(&self) -> &BTreeMap<String, String> {
        &self.metadata
    }

    /// Validates the complete manifest and all cross-field invariants.
    pub fn validate(&self) -> ContractResult<()> {
        if self.manifest_version != APPLICATION_MANIFEST_VERSION {
            return Err(ContractError::InvalidValue {
                field: "manifest_version",
                reason: "unsupported application manifest version",
            });
        }
        validate_text("application_version", &self.application_version, 64)?;
        validate_text("display_name", &self.display_name, 256)?;
        validate_text("vendor", &self.vendor, 256)?;
        self.runtime.validate()?;
        self.jobs.validate()?;
        self.scheduler.validate()?;
        self.health.validate()?;
        self.update.validate()?;

        ensure_unique(
            "capability",
            self.capabilities.iter().map(|item| item.id().as_str()),
        )?;
        for capability in &self.capabilities {
            capability.validate()?;
            reject_reserved_capability_namespace(capability.id().as_str())?;
            if capability.class() != CapabilityClass::Functional {
                return Err(ContractError::InvalidValue {
                    field: "capabilities.class",
                    reason: "application manifests may declare only functional capabilities",
                });
            }
        }
        ensure_unique(
            "leadership.service_id",
            self.leadership
                .iter()
                .map(|item| item.service_id().as_str()),
        )?;
        for leadership in &self.leadership {
            leadership.validate()?;
        }
        if self
            .capabilities
            .iter()
            .any(CapabilityDeclaration::requires_leader)
            && !self.leadership.iter().any(|requirement| {
                requirement.service_id() == &self.service_id
                    && requirement.mode() != LeadershipMode::Disabled
            })
        {
            return Err(ContractError::InvalidValue {
                field: "capabilities.requires_leader",
                reason: "primary service leadership must be enabled",
            });
        }
        ensure_unique(
            "dependency.application_id",
            self.dependencies
                .iter()
                .map(|item| item.application_id().as_str()),
        )?;
        for dependency in &self.dependencies {
            dependency.validate()?;
        }
        ensure_unique("module", self.modules.iter().map(|item| item.id().as_str()))?;
        for module in &self.modules {
            module.validate()?;
        }
        for (key, value) in &self.metadata {
            validate_application_metadata(key, value)?;
        }
        Ok(())
    }
}

fn reject_reserved_capability_namespace(capability: &str) -> ContractResult<()> {
    const RESERVED_PREFIXES: [&str; 3] = ["appcore.", "runtime.", "infrastructure."];
    let normalized = capability.to_ascii_lowercase();
    if RESERVED_PREFIXES
        .iter()
        .any(|prefix| normalized.starts_with(prefix))
    {
        return Err(ContractError::InvalidValue {
            field: "capabilities.id",
            reason: "application capability uses a reserved Runtime namespace",
        });
    }
    Ok(())
}

impl TryFrom<ApplicationManifestData> for ApplicationManifestV1 {
    type Error = ContractError;

    fn try_from(data: ApplicationManifestData) -> Result<Self, Self::Error> {
        let manifest = Self {
            manifest_version: data.manifest_version,
            application_id: data.application_id,
            application_version: data.application_version,
            display_name: data.display_name,
            vendor: data.vendor,
            service_id: data.service_id,
            runtime: data.runtime,
            capabilities: data.capabilities,
            leadership: data.leadership,
            jobs: data.jobs,
            dependencies: data.dependencies,
            storage: data.storage,
            scheduler: data.scheduler,
            health: data.health,
            update: data.update,
            modules: data.modules,
            feature_flags: data.feature_flags,
            metadata: data.metadata,
        };
        manifest.validate()?;
        Ok(manifest)
    }
}

fn ensure_unique<'a>(
    field: &'static str,
    values: impl IntoIterator<Item = &'a str>,
) -> ContractResult<()> {
    let mut seen = BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(ContractError::Duplicate {
                field,
                value: value.to_string(),
            });
        }
    }
    Ok(())
}

fn validate_application_metadata(key: &str, value: &str) -> ContractResult<()> {
    validate_text("metadata.key", key, 128)?;
    validate_text("metadata.value", value, 2_048)?;
    if is_sensitive_key(key) {
        return Err(ContractError::SecretValue {
            field: format!("metadata.{key}"),
        });
    }
    if key.to_ascii_lowercase().contains("path") || looks_like_local_path(value) {
        return Err(ContractError::LocalPath {
            field: format!("metadata.{key}"),
        });
    }
    let is_location_key = key.split(['.', '_', '-']).any(|part| {
        matches!(
            part.to_ascii_lowercase().as_str(),
            "url" | "uri" | "endpoint"
        )
    });
    if is_location_key || looks_like_url(value) {
        return Err(ContractError::InvalidValue {
            field: "metadata",
            reason: "installation-specific URLs belong to the deployment manifest",
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "application/tests.rs"]
mod tests;
