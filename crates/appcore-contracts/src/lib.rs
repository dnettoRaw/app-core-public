// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Stable, implementation-independent contracts for AppCore hosts and applications.
//!
//! The three manifest families intentionally describe different owners:
//! applications publish [`ApplicationManifestV1`], installations provide
//! [`DeploymentManifestV1`], and a running host produces [`RuntimeManifestV1`].

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod application;
mod deployment;
mod error;
mod identifiers;
mod policy;
mod runtime;

pub use application::{ApplicationManifestV1, APPLICATION_MANIFEST_VERSION};
pub use deployment::{
    DeploymentManifestBuilder, DeploymentManifestV1, DeploymentSupervisorConfig,
    DeploymentWatchdogConfig, EnvironmentBinding, NetworkConfig, ProviderConfig, SecretRef,
    TlsConfig, VolumeMount, DEPLOYMENT_MANIFEST_VERSION,
};
pub use error::{ContractError, ContractResult};
pub use identifiers::{
    ApplicationId, BuildId, CapabilityId, CoreId, FeatureId, InstallationId, JobId, ModuleId,
    NodeId, ProviderId, ServiceId,
};
pub use policy::{
    ApplicationDependency, CapabilityClass, CapabilityDeclaration, CapabilityMode,
    CapabilityVisibility, CoreProfile, CoreRole, HealthRequirements, JobPolicy, LeadershipMode,
    LeadershipRequirement, ModuleDeclaration, ResourceProfile, RuntimeMode, RuntimeRequirements,
    SchedulerRequirements, SchedulingProfile, StorageDurability, StorageRequirements, UpdatePolicy,
    WorkloadClass,
};
pub use runtime::{
    RuntimeHealth, RuntimeHealthStatus, RuntimeManifestV1, RuntimeOperationalMode,
    RUNTIME_MANIFEST_VERSION,
};
