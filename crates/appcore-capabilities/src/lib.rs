// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/20 23:03:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/22 15:41:18 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Generic capability registry and resolver.
//!
//! This crate routes capability requests. It intentionally does not know SQL,
//! schemas, customer data, or business rules.

#![deny(missing_docs)]

mod catalog;
mod contract;
mod peer_rpc_invoker;
mod registry;
mod resolver;
mod selection;

pub use contract::{
    CapabilityError, CapabilityRequest, CapabilityResponse, CapabilityResult,
    LocalCapabilityHandler, RemoteCapabilityInvoker,
};
pub use peer_rpc_invoker::PeerRpcRemoteCapabilityInvoker;
pub use registry::{CapabilityRegistry, LocalCapabilityProvider};
pub use resolver::{requirements_for_read_only, CapabilityResolver};
pub use selection::{
    CapabilityProvider, CapabilitySelectionPolicy, DefaultCapabilitySelectionPolicy,
    ResolutionPolicy,
};

mod policy;
#[cfg(test)]
mod tests;
pub use catalog::{CapabilityCatalog, CapabilityEnforcementContext};
