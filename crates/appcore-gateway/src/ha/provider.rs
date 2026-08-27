// =============================================================================
//        #######
//     ###       ###     F: provider.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Provider-independent asynchronous ownership and fencing operations.

use super::types::{
    GatewayInstanceLease, GatewayRegistryResult, GatewayRequestFence, GatewaySessionRecord,
    GatewayWorkerRecord, GatewayWorkerRegistration,
};
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};
use std::future::Future;
use std::pin::Pin;

/// Boxed provider operation used by object-safe Gateway HA implementations.
pub type GatewayRegistryFuture<'a, T> =
    Pin<Box<dyn Future<Output = GatewayRegistryResult<T>> + Send + 'a>>;

/// Shared Gateway ownership, discovery and in-flight request registry.
///
/// Implementations must atomically compare tenant-local instance fencing before
/// mutating worker, session or request state. They must never silently fall
/// back to process-local state when the shared provider is unavailable.
pub trait GatewayRegistryProvider: Send + Sync {
    /// Acquires a new tenant-local instance lease with a monotonic epoch.
    fn acquire_instance<'a>(
        &'a self,
        tenant_id: &'a TenantId,
        cluster_id: &'a ClusterId,
        instance_id: &'a InstanceId,
        federation_url: &'a super::types::GatewayFederationUrl,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayInstanceLease>;

    /// Renews one exact current instance lease without changing its epoch.
    fn renew_instance<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayInstanceLease>;

    /// Releases one exact current instance lease without resetting its epoch.
    fn release_instance<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
    ) -> GatewayRegistryFuture<'a, ()>;

    /// Checks that an instance lease is current and unexpired.
    fn check_instance<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, ()>;

    /// Registers or replaces one worker only under a current instance fence.
    fn register_worker<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        registration: GatewayWorkerRegistration,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayWorkerRecord>;

    /// Renews one exact worker generation and owner fence.
    fn renew_worker<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        worker: &'a GatewayWorkerRecord,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayWorkerRecord>;

    /// Removes one exact worker generation and owner fence.
    fn remove_worker<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        worker: &'a GatewayWorkerRecord,
    ) -> GatewayRegistryFuture<'a, ()>;

    /// Resolves one live fenced worker by tenant, cluster and Core identity.
    fn resolve_worker<'a>(
        &'a self,
        tenant_id: &'a TenantId,
        cluster_id: &'a ClusterId,
        core_id: &'a CoreId,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, Option<GatewayWorkerRecord>>;

    /// Resolves a bounded set of live fenced workers for one capability.
    fn resolve_capability<'a>(
        &'a self,
        tenant_id: &'a TenantId,
        capability: &'a CapabilityName,
        limit: usize,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, Vec<GatewayWorkerRecord>>;

    /// Registers or replaces one authenticated client session under a fence.
    fn register_session<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        session: GatewaySessionRecord,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewaySessionRecord>;

    /// Removes one exact session owner fence.
    fn remove_session<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        session: &'a GatewaySessionRecord,
    ) -> GatewayRegistryFuture<'a, ()>;

    /// Claims one bounded request under current origin and target fences.
    fn claim_request<'a>(
        &'a self,
        origin: &'a GatewayInstanceLease,
        target: &'a GatewayWorkerRecord,
        request_id: &'a str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayRequestFence>;

    /// Checks one request claim and both owner/worker fences without consuming it.
    fn check_request<'a>(
        &'a self,
        request: &'a GatewayRequestFence,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, ()>;

    /// Completes and removes one request only while both fences remain current.
    fn complete_request<'a>(
        &'a self,
        request: &'a GatewayRequestFence,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, ()>;

    /// Cancels one request under its exact origin fence.
    fn cancel_request<'a>(
        &'a self,
        request: &'a GatewayRequestFence,
    ) -> GatewayRegistryFuture<'a, ()>;
}
