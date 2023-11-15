// =============================================================================
//        #######
//     ###       ###     F: context.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime context contract available to runtime components.

use crate::ids::{AppFamily, AppId, NodeId, RuntimeContractVersion, SyncGroup};

/// Minimal read-only runtime context contract.
pub trait RuntimeContext: Send + Sync {
    /// Returns the hosted application identity.
    fn app_id(&self) -> &AppId;
    /// Returns the application compatibility family.
    fn app_family(&self) -> &AppFamily;
    /// Returns the synchronization isolation group.
    fn sync_group(&self) -> &SyncGroup;
    /// Returns the Runtime contract version.
    fn runtime_contract(&self) -> RuntimeContractVersion;
    /// Returns the current node identity.
    fn node_id(&self) -> &NodeId;
}
