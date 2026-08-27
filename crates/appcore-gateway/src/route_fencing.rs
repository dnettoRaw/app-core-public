// =============================================================================
//        #######
//     ###       ###     F: route_fencing.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Optional shared request fencing around the existing local route path.

use crate::{
    GatewayLocalRequestClaim, GatewayRegistryResult, GatewayRequestFence, GatewayState,
    WorkerConnection,
};
use appcore_types::{ClusterId, TenantId};
use std::sync::Arc;

pub(crate) struct PendingCleanup {
    state: Arc<GatewayState>,
    tenant_id: TenantId,
    request_id: String,
}

impl PendingCleanup {
    pub(crate) fn new(state: Arc<GatewayState>, tenant_id: TenantId, request_id: String) -> Self {
        Self {
            state,
            tenant_id,
            request_id,
        }
    }
}

impl Drop for PendingCleanup {
    fn drop(&mut self) {
        cleanup_pending(&self.state, &self.tenant_id, &self.request_id);
    }
}

pub(crate) fn cleanup_pending(state: &GatewayState, tenant_id: &TenantId, request_id: &str) {
    if let Some(tenant_state) = state.tenant_partition(tenant_id) {
        tenant_state.write().remove_pending_request(request_id);
    }
}

pub(crate) async fn claim_route(
    state: &GatewayState,
    tenant_id: &TenantId,
    cluster_id: &ClusterId,
    worker: &WorkerConnection,
    request_id: &str,
    expires_at_ms: u64,
    now_ms: u64,
) -> GatewayRegistryResult<Option<GatewayRequestFence>> {
    match state.ha_coordinator() {
        Some(coordinator) => coordinator
            .claim_local_request(GatewayLocalRequestClaim {
                tenant_id,
                cluster_id,
                core_id: &worker.key.core_id,
                worker_generation: worker.generation(),
                request_id,
                expires_at_ms,
                now_ms,
            })
            .await
            .map(Some),
        None => Ok(None),
    }
}

pub(crate) async fn complete_route(
    state: &GatewayState,
    fence: &Option<GatewayRequestFence>,
    now_ms: u64,
) -> GatewayRegistryResult<()> {
    match (state.ha_coordinator(), fence) {
        (Some(coordinator), Some(fence)) => coordinator.complete_request(fence, now_ms).await,
        _ => Ok(()),
    }
}

pub(crate) async fn cancel_route(
    state: &GatewayState,
    fence: &Option<GatewayRequestFence>,
) -> GatewayRegistryResult<()> {
    match (state.ha_coordinator(), fence) {
        (Some(coordinator), Some(fence)) => coordinator.cancel_request(fence).await,
        _ => Ok(()),
    }
}
