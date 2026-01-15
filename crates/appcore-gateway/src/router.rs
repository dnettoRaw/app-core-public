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
use appcore_peer_rpc::payload_hash;
use appcore_types::{ProtocolVersion, TenantId};
use axum::extract::ws::Message;
use std::collections::HashMap;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PendingKey {
    state_id: usize,
    tenant_id: String,
    request_id: String,
    mesh: bool,
}

#[derive(Debug, Clone, Copy)]
struct PendingMetadata {
    worker_generation: u64,
    response_limit: Option<usize>,
}

// appcore-norm: allow(global-state) reason: gateway callbacks require process-wide pending request correlation
static PENDING_METADATA: LazyLock<Mutex<HashMap<PendingKey, PendingMetadata>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

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
        let request_id = envelope.request_id.clone();
        let tenant_id = envelope.tenant_id.clone();
        let capability = envelope.capability.clone();
        if let Some(error) = validate_routing_envelope(&envelope) {
            state.metrics.routing_failure();
            return PeerRpcResponse::rejected(request_id, error);
        }
        let timeout = timeout.min(crate::config::MAX_GATEWAY_REQUEST_TIMEOUT);

        // 1. Resolve target worker and register pending channel
        let (rx, worker_conn) = {
            let mut tenants = state.tenants.write();
            let tenant_state = tenants
                .entry(tenant_id.clone())
                .or_insert_with(|| crate::tenant::TenantState::new(tenant_id.clone()));

            let Some(worker_conn) = tenant_state
                .get_worker_by_core(&envelope.target_core_id)
                .filter(|worker| {
                    worker.cluster_id() == Some(&envelope.cluster_id)
                        && tenant_state
                            .registry
                            .resolve(&capability)
                            .is_some_and(|workers| workers.contains(&worker.key))
                })
                .cloned()
            else {
                state.metrics.routing_failure();
                return PeerRpcResponse::rejected(
                    request_id,
                    format!("compatible_worker_unavailable: {}", capability.as_str()),
                );
            };
            if !tenant_state.can_register_pending(&request_id, false) {
                state.metrics.routing_failure();
                return PeerRpcResponse::rejected(request_id, "pending_request_rejected");
            }

            let (tx, rx) = oneshot::channel();
            tenant_state.pending_requests.insert(request_id.clone(), tx);
            register_pending_metadata(
                &state,
                &tenant_id,
                &request_id,
                false,
                worker_conn.generation(),
                None,
            );
            (rx, worker_conn)
        };
        let _cleanup = PendingCleanup::new(
            Arc::clone(&state),
            tenant_id.clone(),
            request_id.clone(),
            false,
        );

        // 2. Forward the envelope to the worker
        let payload = match serde_json::to_string(&envelope) {
            Ok(json) => json,
            Err(err) => {
                state.metrics.routing_failure();
                cleanup_pending_request(&state, &tenant_id, &request_id);
                return PeerRpcResponse::rejected(
                    request_id,
                    format!("serialization_failed: {err}"),
                );
            }
        };

        if let Err(err) = worker_conn.send(Message::Text(payload.into())) {
            state.metrics.routing_failure();
            cleanup_pending_request(&state, &tenant_id, &request_id);
            return PeerRpcResponse::rejected(request_id, format!("forward_failed: {err:?}"));
        }

        // 3. Await worker response or timeout
        tokio::select! {
            biased;
            _ = state.wait_for_shutdown() => {
                state.metrics.routing_failure();
                PeerRpcResponse::rejected(request_id, "gateway_shutting_down")
            }
            result = tokio::time::timeout(timeout, rx) => match result {
                Ok(Ok(response)) => {
                    state.metrics.message_routed();
                    response
                }
                Ok(Err(_)) => {
                    state.metrics.routing_failure();
                    PeerRpcResponse::rejected(request_id, "worker_connection_lost")
                }
                Err(_) => {
                    state.metrics.routing_failure();
                    PeerRpcResponse::rejected(request_id, "worker_response_timeout")
                }
            }
        }
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
        if let Err(error) = request.validate_schema() {
            state.metrics.routing_failure();
            return MeshPeerResponse::rejected(request.request_id, error.to_string());
        }
        let request_id = request.request_id.clone();
        let tenant_id = request.target_tenant_id.clone();
        let timeout = timeout.min(crate::config::MAX_GATEWAY_REQUEST_TIMEOUT);
        let peer_envelope = request.peer_envelope().ok();

        let (rx, worker_conn) = {
            let mut tenants = state.tenants.write();
            let Some(tenant_state) = tenants.get_mut(&tenant_id) else {
                state.metrics.routing_failure();
                return MeshPeerResponse::rejected(request_id, "tenant_unavailable");
            };
            let Some(worker_conn) = tenant_state
                .get_worker_by_core(&request.target_core_id)
                .filter(|worker| {
                    peer_envelope.as_ref().is_none_or(|envelope| {
                        worker.cluster_id() == Some(&envelope.cluster_id)
                            && tenant_state
                                .registry
                                .resolve(&envelope.capability)
                                .is_some_and(|workers| workers.contains(&worker.key))
                    })
                })
                .cloned()
            else {
                state.metrics.routing_failure();
                return MeshPeerResponse::rejected(request_id, "worker_offline");
            };
            if !tenant_state.can_register_pending(&request_id, true) {
                state.metrics.routing_failure();
                return MeshPeerResponse::rejected(request_id, "pending_request_rejected");
            }
            let (tx, rx) = oneshot::channel();
            tenant_state
                .pending_mesh_requests
                .insert(request_id.clone(), tx);
            register_pending_metadata(
                &state,
                &tenant_id,
                &request_id,
                true,
                worker_conn.generation(),
                Some(request.max_response_bytes),
            );
            (rx, worker_conn)
        };
        let _cleanup = PendingCleanup::new(
            Arc::clone(&state),
            tenant_id.clone(),
            request_id.clone(),
            true,
        );

        let payload = match serde_json::to_string(&request) {
            Ok(json) => json,
            Err(error) => {
                state.metrics.routing_failure();
                cleanup_pending_mesh_request(&state, &tenant_id, &request_id);
                return MeshPeerResponse::rejected(
                    request_id,
                    format!("serialization_failed: {error}"),
                );
            }
        };
        if let Err(error) = worker_conn.send(Message::Text(payload.into())) {
            state.metrics.routing_failure();
            cleanup_pending_mesh_request(&state, &tenant_id, &request_id);
            return MeshPeerResponse::rejected(request_id, format!("forward_failed: {error:?}"));
        }

        tokio::select! {
            biased;
            _ = state.wait_for_shutdown() => {
                state.metrics.routing_failure();
                MeshPeerResponse::rejected(request_id, "gateway_shutting_down")
            }
            result = tokio::time::timeout(timeout, rx) => match result {
                Ok(Ok(response)) => {
                    state.metrics.message_routed();
                    response
                }
                Ok(Err(_)) => {
                    state.metrics.routing_failure();
                    MeshPeerResponse::rejected(request_id, "worker_connection_lost")
                }
                Err(_) => {
                    state.metrics.routing_failure();
                    MeshPeerResponse::rejected(request_id, "worker_response_timeout")
                }
            }
        }
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

struct PendingCleanup {
    state: Arc<GatewayState>,
    tenant_id: TenantId,
    request_id: String,
    mesh: bool,
}

impl PendingCleanup {
    fn new(state: Arc<GatewayState>, tenant_id: TenantId, request_id: String, mesh: bool) -> Self {
        Self {
            state,
            tenant_id,
            request_id,
            mesh,
        }
    }
}

impl Drop for PendingCleanup {
    fn drop(&mut self) {
        if self.mesh {
            cleanup_pending_mesh_request(&self.state, &self.tenant_id, &self.request_id);
        } else {
            cleanup_pending_request(&self.state, &self.tenant_id, &self.request_id);
        }
    }
}

fn cleanup_pending_request(state: &GatewayState, tenant_id: &TenantId, request_id: &str) {
    let mut tenants = state.tenants.write();
    if let Some(tenant_state) = tenants.get_mut(tenant_id) {
        tenant_state.pending_requests.remove(request_id);
    }
    remove_pending_metadata(state, tenant_id, request_id, false);
}

fn cleanup_pending_mesh_request(state: &GatewayState, tenant_id: &TenantId, request_id: &str) {
    let mut tenants = state.tenants.write();
    if let Some(tenant_state) = tenants.get_mut(tenant_id) {
        tenant_state.pending_mesh_requests.remove(request_id);
    }
    remove_pending_metadata(state, tenant_id, request_id, true);
}

fn dispatch_worker_response(
    state: Arc<GatewayState>,
    tenant_id: &TenantId,
    generation: Option<u64>,
    response: PeerRpcResponse,
) -> GatewayResult<()> {
    let request_id = response.request_id.clone();
    let mut tenants = state.tenants.write();
    if let Some(tenant_state) = tenants.get_mut(tenant_id) {
        let expected = pending_metadata(&state, tenant_id, &request_id, false);
        if expected.is_some_and(|metadata| {
            generation.is_none_or(|value| value == metadata.worker_generation)
        }) {
            remove_pending_metadata(&state, tenant_id, &request_id, false);
            if let Some(tx) = tenant_state.pending_requests.remove(&request_id) {
                let _ = tx.send(response);
                return Ok(());
            }
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
    let request_id = response.request_id.clone();
    let mut tenants = state.tenants.write();
    if let Some(tenant_state) = tenants.get_mut(tenant_id) {
        let expected = pending_metadata(&state, tenant_id, &request_id, true);
        if expected.is_some_and(|metadata| {
            generation.is_none_or(|value| value == metadata.worker_generation)
                && metadata
                    .response_limit
                    .is_some_and(|limit| response.validate_for_request(&request_id, limit).is_ok())
        }) {
            remove_pending_metadata(&state, tenant_id, &request_id, true);
            if let Some(tx) = tenant_state.pending_mesh_requests.remove(&request_id) {
                let _ = tx.send(response);
                return Ok(());
            }
        }
    }
    Err(orphaned_response(tenant_id, &request_id, true))
}

fn register_pending_metadata(
    state: &GatewayState,
    tenant_id: &TenantId,
    request_id: &str,
    mesh: bool,
    worker_generation: u64,
    response_limit: Option<usize>,
) {
    pending_metadata_map().insert(
        pending_key(state, tenant_id, request_id, mesh),
        PendingMetadata {
            worker_generation,
            response_limit,
        },
    );
}

fn pending_metadata(
    state: &GatewayState,
    tenant_id: &TenantId,
    request_id: &str,
    mesh: bool,
) -> Option<PendingMetadata> {
    pending_metadata_map()
        .get(&pending_key(state, tenant_id, request_id, mesh))
        .copied()
}

fn remove_pending_metadata(
    state: &GatewayState,
    tenant_id: &TenantId,
    request_id: &str,
    mesh: bool,
) {
    pending_metadata_map().remove(&pending_key(state, tenant_id, request_id, mesh));
}

fn pending_metadata_map() -> std::sync::MutexGuard<'static, HashMap<PendingKey, PendingMetadata>> {
    PENDING_METADATA
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn pending_key(
    state: &GatewayState,
    tenant_id: &TenantId,
    request_id: &str,
    mesh: bool,
) -> PendingKey {
    PendingKey {
        state_id: std::ptr::from_ref(state) as usize,
        tenant_id: tenant_id.as_str().to_string(),
        request_id: request_id.to_string(),
        mesh,
    }
}

fn orphaned_response(tenant_id: &TenantId, request_id: &str, mesh: bool) -> GatewayError {
    GatewayError::Protocol(format!(
        "orphaned {}response or timeout for tenant {} request: {}",
        if mesh { "worker mesh " } else { "worker " },
        tenant_id.as_str(),
        request_id
    ))
}

fn validate_routing_envelope(envelope: &PeerRpcEnvelope) -> Option<&'static str> {
    if envelope.protocol_version != ProtocolVersion::default() {
        return Some("protocol_version_unsupported");
    }
    if envelope.expires_at_ms <= envelope.timestamp_ms {
        return Some("envelope_expiry_invalid");
    }
    if envelope.expires_at_ms <= now_ms() {
        return Some("envelope_expired");
    }
    if envelope.body_hash != payload_hash(&envelope.payload) {
        return Some("body_hash_invalid");
    }
    if envelope.payload.len() > crate::config::MAX_GATEWAY_MESSAGE_BYTES {
        return Some("payload_too_large");
    }
    None
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
