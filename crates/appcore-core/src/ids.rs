// =============================================================================
//        #######
//     ###       ###     F: ids.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Foundational Runtime identifier exports.

pub use appcore_types::{
    validate_distributed_identifier, validate_identifier, AppFamily, AppId, CapabilityName,
    ClusterId, CommandName, CoreId, EventName, InstanceId, NodeId, ProtocolVersion, QueryName,
    RuntimeContractVersion, StateName, SyncGroup, TenantId,
};

#[cfg(test)]
#[path = "ids_tests.rs"]
mod tests;
