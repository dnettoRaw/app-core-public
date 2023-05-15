// =============================================================================
//        #######
//     ###       ###     F: policy.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 10:59:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Generic application requirements and runtime scheduling profiles.

use crate::identifiers::validate_text;
use crate::{
    ApplicationId, CapabilityId, ContractError, ContractResult, FeatureId, ModuleId, ServiceId,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod capability;
mod dependency;
mod health;
mod job;
mod leadership;
mod module;
mod profile;
mod runtime;
mod scheduler;
mod storage;
mod update;

pub use capability::{
    CapabilityClass, CapabilityDeclaration, CapabilityVisibility, RuntimeRequirements,
};
pub use dependency::ApplicationDependency;
pub use health::HealthRequirements;
pub use job::JobPolicy;
pub use leadership::{LeadershipMode, LeadershipRequirement};
pub use module::ModuleDeclaration;
pub use profile::{CoreProfile, CoreRole, ResourceProfile, SchedulingProfile, WorkloadClass};
pub use runtime::{CapabilityMode, RuntimeMode};
pub use scheduler::SchedulerRequirements;
pub use storage::{StorageDurability, StorageRequirements};
pub use update::UpdatePolicy;

#[cfg(test)]
mod tests;
