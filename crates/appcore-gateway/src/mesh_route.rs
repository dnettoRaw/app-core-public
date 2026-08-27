// =============================================================================
//        #######
//     ###       ###     F: mesh_route.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 1.0.4-rc
// =============================================================================

//! Bounded local and federated mesh route phases.

use crate::connection::WorkerSendFailure;
use crate::federated_route::forward_remote_mesh_request;
use crate::metrics::RouteObservation;
use crate::route_admission::{admit_mesh_route, MeshRouteAdmission};
use crate::route_fencing::{cancel_route, claim_route, complete_route, PendingCleanup};
use crate::telemetry::RouteOutcome;
use crate::{
    GatewayRequestFence, GatewayState, GatewayWorkerRecord, MeshPeerRequest, MeshPeerResponse,
    WorkerConnection,
};
use appcore_distributed_contracts::PeerRpcEnvelope;
use axum::extract::ws::Message;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

pub(crate) async fn route_mesh_request(
    state: Arc<GatewayState>,
    request: MeshPeerRequest,
    timeout: Duration,
) -> MeshPeerResponse {
    let mut telemetry = state.metrics.route_started(None, request.body.len());
    if state.admit_ha_tenant(&request.target_tenant_id).is_err() {
        return reject_mesh(
            &state,
            &mut telemetry,
            request.request_id,
            "registry_unavailable",
            RouteOutcome::TransportFailure,
        );
    }
    if let Err(error) = request.validate_schema() {
        return reject_mesh(
            &state,
            &mut telemetry,
            request.request_id,
            error.to_string(),
            RouteOutcome::Invalid,
        );
    }
    let timeout = timeout.min(crate::config::MAX_GATEWAY_REQUEST_TIMEOUT);
    let peer_envelope = request.peer_envelope().ok();
    if let Some(envelope) = &peer_envelope {
        telemetry.set_capability(&envelope.capability);
    }
    match resolve_remote_target(&state, &request, peer_envelope.as_ref(), &mut telemetry).await {
        Ok(Some(target)) => {
            forward_remote_mesh_request(state, request, target, timeout, &mut telemetry).await
        }
        Ok(None) => route_local_mesh(state, request, peer_envelope, timeout, telemetry).await,
        Err(response) => response,
    }
}

async fn resolve_remote_target(
    state: &GatewayState,
    request: &MeshPeerRequest,
    envelope: Option<&PeerRpcEnvelope>,
    telemetry: &mut RouteObservation,
) -> Result<Option<GatewayWorkerRecord>, MeshPeerResponse> {
    let Some(coordinator) = state.ha_coordinator() else {
        return Ok(None);
    };
    let local_lease = coordinator
        .lease_for(&request.target_tenant_id)
        .map_err(|_| {
            reject_mesh(
                state,
                telemetry,
                &request.request_id,
                "registry_unavailable",
                RouteOutcome::TransportFailure,
            )
        })?;
    let cluster_id = envelope.map_or_else(|| local_lease.cluster_id(), |value| &value.cluster_id);
    let target = coordinator
        .resolve_worker(
            &request.target_tenant_id,
            cluster_id,
            &request.target_core_id,
            now_ms(),
        )
        .await
        .map_err(|_| {
            reject_mesh(
                state,
                telemetry,
                &request.request_id,
                "registry_unavailable",
                RouteOutcome::TransportFailure,
            )
        })?
        .ok_or_else(|| {
            reject_mesh(
                state,
                telemetry,
                &request.request_id,
                "worker_offline",
                RouteOutcome::WorkerUnavailable,
            )
        })?;
    Ok((target.owner != local_lease).then_some(target))
}

async fn route_local_mesh(
    state: Arc<GatewayState>,
    request: MeshPeerRequest,
    peer_envelope: Option<PeerRpcEnvelope>,
    timeout: Duration,
    mut telemetry: RouteObservation,
) -> MeshPeerResponse {
    let request_id = request.request_id.clone();
    let tenant_id = request.target_tenant_id.clone();
    let MeshRouteAdmission {
        receiver,
        worker,
        permit: _permit,
    } = match admit_mesh_route(&state, &request, Instant::now() + timeout, &mut telemetry) {
        Ok(admission) => admission,
        Err(rejection) => {
            return reject_mesh(
                &state,
                &mut telemetry,
                request_id,
                rejection.mesh_error(),
                rejection.outcome(),
            );
        }
    };
    let _cleanup = PendingCleanup::new(Arc::clone(&state), tenant_id.clone(), request_id.clone());
    let Some(cluster_id) = worker.cluster_id().cloned() else {
        return reject_mesh(
            &state,
            &mut telemetry,
            request_id,
            "registry_unavailable",
            RouteOutcome::TransportFailure,
        );
    };
    let claimed_at_ms = now_ms();
    let expires_at_ms = peer_envelope.as_ref().map_or_else(
        || claimed_at_ms.saturating_add(request.timeout_ms),
        |envelope| {
            envelope
                .expires_at_ms
                .min(claimed_at_ms.saturating_add(request.timeout_ms))
        },
    );
    let fence = match claim_route(
        &state,
        &tenant_id,
        &cluster_id,
        &worker,
        &request_id,
        expires_at_ms,
        claimed_at_ms,
    )
    .await
    {
        Ok(fence) => fence,
        Err(_) => {
            return reject_mesh(
                &state,
                &mut telemetry,
                request_id,
                "registry_unavailable",
                RouteOutcome::TransportFailure,
            );
        }
    };
    if let Err(response) = send_mesh_request(
        &state,
        &request,
        &worker,
        &fence,
        &tenant_id,
        &request_id,
        &mut telemetry,
    )
    .await
    {
        return response;
    }
    await_mesh_response(state, receiver, fence, request_id, timeout, telemetry).await
}

async fn send_mesh_request(
    state: &GatewayState,
    request: &MeshPeerRequest,
    worker: &WorkerConnection,
    fence: &Option<GatewayRequestFence>,
    tenant_id: &appcore_types::TenantId,
    request_id: &str,
    telemetry: &mut RouteObservation,
) -> Result<(), MeshPeerResponse> {
    let payload = match serde_json::to_string(request) {
        Ok(json) => json,
        Err(error) => {
            crate::route_fencing::cleanup_pending(state, tenant_id, request_id);
            let _ = cancel_route(state, fence).await;
            return Err(reject_mesh(
                state,
                telemetry,
                request_id,
                format!("serialization_failed: {error}"),
                RouteOutcome::Invalid,
            ));
        }
    };
    telemetry.observe_queue_depth(worker.outbound_queue_depth());
    if let Err(error) = worker.send_routed(Message::Text(payload.into())) {
        crate::route_fencing::cleanup_pending(state, tenant_id, request_id);
        let _ = cancel_route(state, fence).await;
        let outcome = if error == WorkerSendFailure::Saturated {
            RouteOutcome::QueueSaturation
        } else {
            RouteOutcome::TransportFailure
        };
        return Err(reject_mesh(
            state,
            telemetry,
            request_id,
            "forward_failed",
            outcome,
        ));
    }
    telemetry.dispatched();
    Ok(())
}

async fn await_mesh_response(
    state: Arc<GatewayState>,
    receiver: oneshot::Receiver<MeshPeerResponse>,
    fence: Option<GatewayRequestFence>,
    request_id: String,
    timeout: Duration,
    mut telemetry: RouteObservation,
) -> MeshPeerResponse {
    tokio::select! {
        biased;
        _ = state.wait_for_shutdown() => {
            let _ = cancel_route(&state, &fence).await;
            reject_mesh(&state, &mut telemetry, request_id, "gateway_shutting_down", RouteOutcome::Shutdown)
        }
        result = tokio::time::timeout(timeout, receiver) => match result {
            Ok(Ok(response)) => {
                if complete_route(&state, &fence, now_ms()).await.is_err() {
                    return reject_mesh(&state, &mut telemetry, request_id, "registry_unavailable", RouteOutcome::TransportFailure);
                }
                state.metrics.message_routed();
                telemetry.finish(RouteOutcome::Success);
                response
            }
            Ok(Err(_)) => {
                let _ = cancel_route(&state, &fence).await;
                reject_mesh(&state, &mut telemetry, request_id, "worker_connection_lost", RouteOutcome::TransportFailure)
            }
            Err(_) => {
                let _ = cancel_route(&state, &fence).await;
                reject_mesh(&state, &mut telemetry, request_id, "worker_response_timeout", RouteOutcome::Timeout)
            }
        }
    }
}

fn reject_mesh(
    state: &GatewayState,
    telemetry: &mut RouteObservation,
    request_id: impl Into<String>,
    error: impl Into<String>,
    outcome: RouteOutcome,
) -> MeshPeerResponse {
    state.metrics.routing_failure();
    telemetry.finish(outcome);
    MeshPeerResponse::rejected(request_id, error)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
