// =============================================================================
//        #######
//     ###       ###     F: federated_route.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.6-rc
// =============================================================================

//! Origin forwarding and target dispatch for preclaimed federation requests.

use crate::connection::WorkerSendFailure;
use crate::federation_auth::mint_federation_token;
use crate::metrics::RouteObservation;
use crate::route_admission::{admit_mesh_route, MeshRouteAdmission, RouteAdmissionRejection};
use crate::telemetry::RouteOutcome;
use crate::{
    GatewayFederationRequestV2, GatewayFederationResponseV2, GatewayRequestFence, GatewayState,
    GatewayWorkerRecord, MeshPeerRequest, MeshPeerResponse,
};
use appcore_peer_rpc::v2::{PeerRpcWireErrorCodeV2, PeerRpcWireErrorV2};
use appcore_types::TenantId;
use axum::extract::ws::Message;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(crate) async fn forward_remote_mesh_request(
    state: Arc<GatewayState>,
    mut request: MeshPeerRequest,
    target: GatewayWorkerRecord,
    timeout: Duration,
    telemetry: &mut RouteObservation,
) -> MeshPeerResponse {
    let request_id = request.request_id.clone();
    let claimed_at_ms = now_ms();
    let expires_at_ms = request.peer_envelope().map_or_else(
        |_| claimed_at_ms.saturating_add(duration_ms(timeout)),
        |envelope| {
            envelope
                .expires_at_ms
                .min(claimed_at_ms.saturating_add(duration_ms(timeout)))
        },
    );
    request.timeout_ms = request
        .timeout_ms
        .min(expires_at_ms.saturating_sub(claimed_at_ms));
    if request.timeout_ms == 0 {
        return remote_rejection(&state, telemetry, request_id);
    }
    let Some(coordinator) = state.ha_coordinator() else {
        return remote_rejection(&state, telemetry, request_id);
    };
    let fence = match coordinator
        .claim_remote_request(&target, &request_id, expires_at_ms, claimed_at_ms)
        .await
    {
        Ok(fence) => fence,
        Err(_) => return remote_rejection(&state, telemetry, request_id),
    };
    let federation = match GatewayFederationRequestV2::new(fence.clone(), request) {
        Ok(request) => request,
        Err(_) => {
            let _ = coordinator.cancel_request(&fence).await;
            return remote_rejection(&state, telemetry, request_id);
        }
    };
    let credential = match mint_federation_token(&state, &federation, claimed_at_ms) {
        Ok(token) => token,
        Err(_) => {
            let _ = coordinator.cancel_request(&fence).await;
            return remote_rejection(&state, telemetry, request_id);
        }
    };
    let transport = state.federation_transport();
    let target_url = target.owner.federation_url().clone();
    let outbound = federation.clone();
    let exchange =
        tokio::task::spawn_blocking(move || transport.send(&target_url, &credential, &outbound));
    let response = tokio::select! {
        biased;
        _ = state.wait_for_shutdown() => {
            let _ = coordinator.cancel_request(&fence).await;
            telemetry.finish(RouteOutcome::Shutdown);
            return MeshPeerResponse::rejected(request_id, "gateway_shutting_down");
        }
        result = exchange => result,
    };
    let response = match response {
        Ok(Ok(response)) => response,
        _ => {
            let _ = coordinator.cancel_request(&fence).await;
            return remote_rejection(&state, telemetry, request_id);
        }
    };
    if coordinator
        .complete_request(&fence, now_ms())
        .await
        .is_err()
    {
        return remote_rejection(&state, telemetry, request_id);
    }
    coordinator.record_remote_forward();
    match (response.response, response.error) {
        (Some(response), None) => {
            state.metrics.message_routed();
            telemetry.finish(RouteOutcome::Success);
            response
        }
        (None, Some(error)) => {
            state.metrics.routing_failure();
            telemetry.finish(if error.retryable {
                RouteOutcome::TransportFailure
            } else {
                RouteOutcome::Invalid
            });
            MeshPeerResponse::rejected(request_id, federation_error_code(error.code))
        }
        _ => remote_rejection(&state, telemetry, request_id),
    }
}

pub(crate) async fn route_claimed_mesh_request(
    state: Arc<GatewayState>,
    request: GatewayFederationRequestV2,
) -> GatewayFederationResponseV2 {
    let fence = request.fence.clone();
    let request_id = request.request.request_id.clone();
    let mut telemetry = state
        .metrics
        .route_started(None, request.request.body.len());
    let remaining_ms = request.fence.expires_at_ms.saturating_sub(now_ms());
    let timeout = Duration::from_millis(request.request.timeout_ms.min(remaining_ms))
        .min(crate::config::MAX_GATEWAY_REQUEST_TIMEOUT);
    if timeout.is_zero() {
        state.metrics.routing_failure();
        telemetry.finish(RouteOutcome::Timeout);
        return reject(fence, request_id, PeerRpcWireErrorCodeV2::Expired);
    }
    let deadline = Instant::now() + timeout;
    let MeshRouteAdmission {
        receiver,
        worker,
        permit: _permit,
    } = match admit_mesh_route(&state, &request.request, deadline, &mut telemetry) {
        Ok(admission) => admission,
        Err(error) => {
            state.metrics.routing_failure();
            telemetry.finish(error.outcome());
            return reject(fence, request_id, admission_error_code(error));
        }
    };
    let _cleanup = PendingCleanup::new(
        Arc::clone(&state),
        request.request.target_tenant_id.clone(),
        request_id.clone(),
    );
    if worker.generation() != fence.worker_generation
        || worker.key.core_id != fence.target_core_id
        || worker.cluster_id() != Some(&fence.target_cluster_id)
    {
        state.metrics.routing_failure();
        telemetry.finish(RouteOutcome::Invalid);
        return reject(fence, request_id, PeerRpcWireErrorCodeV2::TargetMismatch);
    }
    let payload = match serde_json::to_string(&request.request) {
        Ok(payload) => payload,
        Err(_) => {
            state.metrics.routing_failure();
            telemetry.finish(RouteOutcome::Invalid);
            return reject(fence, request_id, PeerRpcWireErrorCodeV2::InvalidFrame);
        }
    };
    telemetry.observe_queue_depth(worker.outbound_queue_depth());
    if let Err(error) = worker.send_routed(Message::Text(payload.into())) {
        state.metrics.routing_failure();
        let code = if error == WorkerSendFailure::Saturated {
            telemetry.finish(RouteOutcome::QueueSaturation);
            PeerRpcWireErrorCodeV2::CapacityExceeded
        } else {
            telemetry.finish(RouteOutcome::TransportFailure);
            PeerRpcWireErrorCodeV2::Io
        };
        return reject(fence, request_id, code);
    }
    telemetry.dispatched();
    tokio::select! {
        biased;
        _ = state.wait_for_shutdown() => {
            state.metrics.routing_failure();
            telemetry.finish(RouteOutcome::Shutdown);
            reject(fence, request_id, PeerRpcWireErrorCodeV2::Cancelled)
        }
        result = tokio::time::timeout(timeout, receiver) => match result {
            Ok(Ok(response)) => {
                state.metrics.message_routed();
                telemetry.finish(RouteOutcome::Success);
                GatewayFederationResponseV2::ok(fence, response)
            }
            Ok(Err(_)) => {
                state.metrics.routing_failure();
                telemetry.finish(RouteOutcome::TransportFailure);
                reject(fence, request_id, PeerRpcWireErrorCodeV2::Io)
            }
            Err(_) => {
                state.metrics.routing_failure();
                telemetry.finish(RouteOutcome::Timeout);
                reject(fence, request_id, PeerRpcWireErrorCodeV2::Expired)
            }
        }
    }
}

struct PendingCleanup {
    state: Arc<GatewayState>,
    tenant_id: TenantId,
    request_id: String,
}

impl PendingCleanup {
    fn new(state: Arc<GatewayState>, tenant_id: TenantId, request_id: String) -> Self {
        Self {
            state,
            tenant_id,
            request_id,
        }
    }
}

impl Drop for PendingCleanup {
    fn drop(&mut self) {
        if let Some(tenant) = self.state.tenant_partition(&self.tenant_id) {
            tenant.write().remove_pending_request(&self.request_id);
        }
    }
}

fn reject(
    fence: GatewayRequestFence,
    request_id: String,
    code: PeerRpcWireErrorCodeV2,
) -> GatewayFederationResponseV2 {
    GatewayFederationResponseV2::rejected(
        fence,
        PeerRpcWireErrorV2::controlled(Some(request_id), None, code),
    )
}

fn admission_error_code(error: RouteAdmissionRejection) -> PeerRpcWireErrorCodeV2 {
    match error {
        RouteAdmissionRejection::WorkerAtCapacity
        | RouteAdmissionRejection::PendingSaturation
        | RouteAdmissionRejection::TenantLimit => PeerRpcWireErrorCodeV2::CapacityExceeded,
        _ => PeerRpcWireErrorCodeV2::EndpointUnavailable,
    }
}

fn remote_rejection(
    state: &GatewayState,
    telemetry: &mut RouteObservation,
    request_id: String,
) -> MeshPeerResponse {
    state.metrics.routing_failure();
    telemetry.finish(RouteOutcome::TransportFailure);
    MeshPeerResponse::rejected(request_id, "federation_unavailable")
}

fn federation_error_code(code: PeerRpcWireErrorCodeV2) -> &'static str {
    match code {
        PeerRpcWireErrorCodeV2::CapacityExceeded => "federation_capacity_exceeded",
        PeerRpcWireErrorCodeV2::Expired => "federation_expired",
        PeerRpcWireErrorCodeV2::Cancelled => "federation_cancelled",
        PeerRpcWireErrorCodeV2::TargetMismatch
        | PeerRpcWireErrorCodeV2::TenantMismatch
        | PeerRpcWireErrorCodeV2::ClusterMismatch
        | PeerRpcWireErrorCodeV2::IdentityMismatch => "federation_identity_mismatch",
        _ => "federation_unavailable",
    }
}

fn duration_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "federated_route_tests.rs"]
mod tests;
