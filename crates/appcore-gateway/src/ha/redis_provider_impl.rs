// =============================================================================
//        #######
//     ###       ###     F: redis_provider_impl.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Object-safe HA contract implementation for the Redis provider.

use super::{
    GatewayFederationUrl, GatewayInstanceLease, GatewayRegistryFuture, GatewayRegistryProvider,
    GatewayRequestFence, GatewaySessionRecord, GatewayWorkerRecord, GatewayWorkerRegistration,
    RedisGatewayRegistryProvider,
};
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};

impl GatewayRegistryProvider for RedisGatewayRegistryProvider {
    fn acquire_instance<'a>(
        &'a self,
        tenant_id: &'a TenantId,
        cluster_id: &'a ClusterId,
        instance_id: &'a InstanceId,
        federation_url: &'a GatewayFederationUrl,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayInstanceLease> {
        Box::pin(self.acquire_instance_inner(
            tenant_id,
            cluster_id,
            instance_id,
            federation_url,
            ttl_ms,
            now_ms,
        ))
    }

    fn renew_instance<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayInstanceLease> {
        Box::pin(self.renew_instance_inner(lease, ttl_ms, now_ms))
    }

    fn release_instance<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(self.release_instance_inner(lease))
    }

    fn check_instance<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(self.check_instance_inner(lease, now_ms))
    }

    fn register_worker<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        registration: GatewayWorkerRegistration,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayWorkerRecord> {
        Box::pin(self.register_worker_inner(lease, registration, ttl_ms, now_ms))
    }

    fn renew_worker<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        worker: &'a GatewayWorkerRecord,
        ttl_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayWorkerRecord> {
        Box::pin(self.renew_worker_inner(lease, worker, ttl_ms, now_ms))
    }

    fn remove_worker<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        worker: &'a GatewayWorkerRecord,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(self.remove_worker_inner(lease, worker))
    }

    fn resolve_worker<'a>(
        &'a self,
        tenant_id: &'a TenantId,
        cluster_id: &'a ClusterId,
        core_id: &'a CoreId,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, Option<GatewayWorkerRecord>> {
        Box::pin(self.resolve_worker_inner(tenant_id, cluster_id, core_id, now_ms))
    }

    fn resolve_capability<'a>(
        &'a self,
        tenant_id: &'a TenantId,
        capability: &'a CapabilityName,
        limit: usize,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, Vec<GatewayWorkerRecord>> {
        Box::pin(self.resolve_capability_inner(tenant_id, capability, limit, now_ms))
    }

    fn register_session<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        session: GatewaySessionRecord,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewaySessionRecord> {
        Box::pin(self.register_session_inner(lease, session, now_ms))
    }

    fn remove_session<'a>(
        &'a self,
        lease: &'a GatewayInstanceLease,
        session: &'a GatewaySessionRecord,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(self.remove_session_inner(lease, session))
    }

    fn claim_request<'a>(
        &'a self,
        origin: &'a GatewayInstanceLease,
        target: &'a GatewayWorkerRecord,
        request_id: &'a str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, GatewayRequestFence> {
        Box::pin(self.claim_request_inner(origin, target, request_id, expires_at_ms, now_ms))
    }

    fn check_request<'a>(
        &'a self,
        request: &'a GatewayRequestFence,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(self.check_request_inner(request, now_ms))
    }

    fn complete_request<'a>(
        &'a self,
        request: &'a GatewayRequestFence,
        now_ms: u64,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(self.complete_request_inner(request, now_ms))
    }

    fn cancel_request<'a>(
        &'a self,
        request: &'a GatewayRequestFence,
    ) -> GatewayRegistryFuture<'a, ()> {
        Box::pin(self.cancel_request_inner(request))
    }
}
