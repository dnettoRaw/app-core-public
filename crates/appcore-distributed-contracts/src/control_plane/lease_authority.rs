// =============================================================================
//        #######
//     ###       ###     F: lease_authority.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:42:56 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.5
// =============================================================================

//! Exact service fencing boundary; authentication belongs to the hosting adapter.
use super::v1::{ControlPlaneFuture, ServiceLeaderLease};
use appcore_contracts::ServiceId;
use appcore_types::CoreIdentity;

/// Authoritative server-clock operations, independent of heartbeat and Gateway HA ownership.
/// Providers must serialize grants, retain fencing history and fail closed on state loss.
/// Check is an observation, not a transaction with a remote application's eventual commit.
pub trait ServiceLeaseAuthority: Send + Sync {
    /// Acquire leadership; an already live grant to the same Core may be returned unchanged.
    /// Cores must be unique writer identities; sharing a Core credential shares its authority.
    fn acquire<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service: &'a ServiceId,
        ttl_ms: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease>;
    /// Renew only the exact currently live grant, never reacquire an expired one.
    fn renew<'a>(
        &'a self,
        lease: &'a ServiceLeaderLease,
        ttl_ms: u64,
    ) -> ControlPlaneFuture<'a, ServiceLeaderLease>;
    /// Release only the exact grant, preserving its epoch tombstone.
    fn release<'a>(&'a self, lease: &'a ServiceLeaderLease) -> ControlPlaneFuture<'a, ()>;
    /// Observe current grant for one tenant/cluster/service, including an expired grant.
    /// Discovery is not permission to write; use check and enforce fencing at the commit boundary.
    fn inspect<'a>(
        &'a self,
        identity: &'a CoreIdentity,
        service: &'a ServiceId,
    ) -> ControlPlaneFuture<'a, Option<ServiceLeaderLease>>;
    /// Reject any changed, missing or expired grant. No offline success is permitted.
    fn check<'a>(&'a self, lease: &'a ServiceLeaderLease) -> ControlPlaneFuture<'a, ()>;
}
