// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Foundational, implementation-independent AppCore Runtime types.
//!
//! This crate owns generic identity, manifest, tracing, operational-mode and
//! error contracts shared by Runtime implementations and versioned wire crates.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod error;
mod identity;
mod ids;
mod manifest;
mod operational;
mod trace;

pub use error::{RuntimeError, RuntimeResult};
pub use identity::{
    CompatibilityStatus, CoreCompatibilityPolicy, CoreCompatibilityStatus, CoreIdentity, CoreKind,
    RuntimeIdentity,
};
pub use ids::{
    validate_distributed_identifier, validate_identifier, AppFamily, AppId, CapabilityName,
    ClusterId, CommandName, CoreId, EventName, InstanceId, NodeId, ProtocolVersion, QueryName,
    RuntimeContractVersion, StateName, SyncGroup, TenantId,
};
pub use manifest::{
    CapabilityDescriptor, CapabilityMode, CapabilityRequirements, CapabilityVisibility,
    DistributedCoreManifest, PeerEndpoint,
};
pub use operational::RuntimeOperationalMode;
pub use trace::TraceContext;
