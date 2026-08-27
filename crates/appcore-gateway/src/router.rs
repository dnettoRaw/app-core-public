// =============================================================================
//        #######
//     ###       ###     F: router.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Multiplexing router for Peer RPC envelopes.

use crate::error::{GatewayError, GatewayResult};
use crate::mesh::{MeshPeerRequest, MeshPeerResponse};
use crate::state::GatewayState;
use appcore_distributed_contracts::{PeerRpcEnvelope, PeerRpcResponse};
use appcore_types::TenantId;
use std::sync::Arc;
use std::time::Duration;

/// Handles routing of envelopes between clients and workers.
pub struct EnvelopeRouter;

impl EnvelopeRouter {
    /// Routes an inbound client request envelope to a worker.
    ///
    /// Resolves the target worker, registers a pending request channel,
    /// forwards the envelope, and awaits the response from the worker.
    pub async fn route_request(
        state: Arc<GatewayState>,
        envelope: PeerRpcEnvelope,
        timeout: Duration,
    ) -> PeerRpcResponse {
        crate::peer_route::route_peer_request(state, envelope, timeout).await
    }

    /// Dispatches a response received from a worker to the waiting client's task.
    pub fn handle_worker_response(
        state: Arc<GatewayState>,
        tenant_id: &TenantId,
        response: PeerRpcResponse,
    ) -> GatewayResult<()> {
        dispatch_worker_response(state, tenant_id, None, response)
    }

    /// Dispatches a response only when it came from the selected worker connection.
    pub fn handle_worker_response_from(
        state: Arc<GatewayState>,
        tenant_id: &TenantId,
        worker: &crate::WorkerConnection,
        response: PeerRpcResponse,
    ) -> GatewayResult<()> {
        dispatch_worker_response(state, tenant_id, Some(worker.generation()), response)
    }

    /// Routes a mesh relay request to the target worker socket.
    pub async fn route_mesh_request(
        state: Arc<GatewayState>,
        request: MeshPeerRequest,
        timeout: Duration,
    ) -> MeshPeerResponse {
        crate::mesh_route::route_mesh_request(state, request, timeout).await
    }

    /// Dispatches a mesh response received from a worker to the waiting relay task.
    pub fn handle_worker_mesh_response(
        state: Arc<GatewayState>,
        tenant_id: &TenantId,
        response: MeshPeerResponse,
    ) -> GatewayResult<()> {
        dispatch_worker_mesh_response(state, tenant_id, None, response)
    }

    /// Dispatches a mesh response only when it came from the selected worker connection.
    pub fn handle_worker_mesh_response_from(
        state: Arc<GatewayState>,
        tenant_id: &TenantId,
        worker: &crate::WorkerConnection,
        response: MeshPeerResponse,
    ) -> GatewayResult<()> {
        dispatch_worker_mesh_response(state, tenant_id, Some(worker.generation()), response)
    }
}

fn dispatch_worker_response(
    state: Arc<GatewayState>,
    tenant_id: &TenantId,
    generation: Option<u64>,
    response: PeerRpcResponse,
) -> GatewayResult<()> {
    if state.admit_ha_tenant(tenant_id).is_err() {
        return Err(GatewayError::Transport(
            "gateway HA registry is unavailable".to_string(),
        ));
    }
    let request_id = response.request_id.clone();
    if let Some(tenant_state) = state.tenant_partition(tenant_id) {
        let mut tenant_state = tenant_state.write();
        if tenant_state.complete_pending_request(&request_id, generation, response) {
            return Ok(());
        }
    }
    Err(orphaned_response(tenant_id, &request_id, false))
}

fn dispatch_worker_mesh_response(
    state: Arc<GatewayState>,
    tenant_id: &TenantId,
    generation: Option<u64>,
    response: MeshPeerResponse,
) -> GatewayResult<()> {
    if state.admit_ha_tenant(tenant_id).is_err() {
        return Err(GatewayError::Transport(
            "gateway HA registry is unavailable".to_string(),
        ));
    }
    let request_id = response.request_id.clone();
    if let Some(tenant_state) = state.tenant_partition(tenant_id) {
        let mut tenant_state = tenant_state.write();
        if tenant_state.complete_pending_mesh_request(&request_id, generation, response) {
            return Ok(());
        }
    }
    Err(orphaned_response(tenant_id, &request_id, true))
}

fn orphaned_response(tenant_id: &TenantId, request_id: &str, mesh: bool) -> GatewayError {
    GatewayError::Protocol(format!(
        "orphaned {}response or timeout for tenant {} request: {}",
        if mesh { "worker mesh " } else { "worker " },
        tenant_id.as_str(),
        request_id
    ))
}
