// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 13:21:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Provider factory and deployment composition contracts.

#![deny(missing_docs)]

mod context;
mod coordination;
mod error;
mod factory;
mod job;
mod plan;
mod role;
mod secret;
mod shared_lease;
mod shared_lease_state;

pub use context::ProviderContext;
pub use coordination::{
    CoordinationStoreProvider, FileCoordinationStore, InMemoryCoordinationStore,
    COORDINATION_SCHEMA_VERSION, COORDINATION_TABLES,
};
pub use error::{ProviderError, ProviderResult};
pub use factory::{ProviderFactory, ProviderRegistry};
pub use job::{JobAtomicity, JobCompletion, JobLease, JobProvider, JobSpec};
pub use plan::DeploymentProviderPlan;
pub use role::ProviderRole;
pub use secret::{ResolvedSecret, SecretProvider};
pub use shared_lease::{
    FileLeaseRepository, LeaseDecision, LeaseHeartbeat, LeaseOwner, LeasePolicy, LeaseRepository,
    LeaseToken, SharedResourceLease,
};

/// Shared coordination-store provider selected by a deployment.
pub type SharedCoordinationStore = std::sync::Arc<dyn CoordinationStoreProvider>;

#[cfg(test)]
mod tests;
