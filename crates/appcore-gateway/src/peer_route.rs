// =============================================================================
//        #######
//     ###       ###     F: peer_route.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Bounded local Peer RPC route phases.

use crate::connection::WorkerSendFailure;
use crate::metrics::RouteObservation;
use crate::route_admission::{admit_peer_route, PeerRouteAdmission};
use crate::route_fencing::{cancel_route, claim_route, complete_route, PendingCleanup};
use crate::telemetry::RouteOutcome;
use crate::{GatewayState, WorkerConnection};
use appcore_distributed_contracts::{PeerRpcEnvelope, PeerRpcResponse};
use appcore_peer_rpc::payload_hash;
use appcore_types::ProtocolVersion;
use axum::extract::ws::Message;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::oneshot;

pub(crate) async fn route_peer_request(
    state: Arc<GatewayState>,
    envelope: PeerRpcEnvelope,
    timeout: Duration,
) -> PeerRpcResponse {
    let request_id = envelope.request_id.clone();
    let tenant_id = envelope.tenant_id.clone();
    let capability = envelope.capability.clone();
    let mut telemetry = state
        .metrics
        .route_started(Some(&capability), envelope.payload.len());
    if state
        .admit_ha_boundary(&envelope.tenant_id, &envelope.cluster_id)
        .is_err()
    {
        return reject_peer(
            &state,
            &mut telemetry,
            request_id,
            "registry_unavailable",
            RouteOutcome::TransportFailure,
        );
    }
    if let Some(error) = validate_routing_envelope(&envelope) {
        return reject_peer(
            &state,
            &mut telemetry,
            request_id,
            error,
            RouteOutcome::Invalid,
        );
    }
    let timeout = timeout.min(crate::config::MAX_GATEWAY_REQUEST_TIMEOUT);
    let deadline = Instant::now() + timeout;
    let PeerRouteAdmission {
        receiver,
        worker,
        permit: _permit,
    } = match admit_peer_route(&state, &envelope, deadline, &mut telemetry) {
        Ok(admission) => admission,
        Err(rejection) => {
            let error = rejection.peer_error(&capability);
            return reject_peer(
                &state,
                &mut telemetry,
                request_id,
                error,
                rejection.outcome(),
            );
        }
    };
    let _cleanup = PendingCleanup::new(Arc::clone(&state), tenant_id.clone(), request_id.clone());
    let claimed_at_ms = now_ms();
    let expires_at_ms = envelope
        .expires_at_ms
        .min(claimed_at_ms.saturating_add(u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX)));
    let fence = match claim_route(
        &state,
        &tenant_id,
        &envelope.cluster_id,
        &worker,
        &request_id,
        expires_at_ms,
        claimed_at_ms,
    )
    .await
    {
        Ok(fence) => fence,
        Err(_) => {
            return reject_peer(
                &state,
                &mut telemetry,
                request_id,
                "registry_unavailable",
                RouteOutcome::TransportFailure,
            );
        }
    };
    if let Err(response) = send_peer_request(
        &state,
        &envelope,
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
    await_peer_response(state, receiver, fence, request_id, timeout, telemetry).await
}

async fn send_peer_request(
    state: &GatewayState,
    envelope: &PeerRpcEnvelope,
    worker: &WorkerConnection,
    fence: &Option<crate::GatewayRequestFence>,
    tenant_id: &appcore_types::TenantId,
    request_id: &str,
    telemetry: &mut RouteObservation,
) -> Result<(), PeerRpcResponse> {
    let payload = match serde_json::to_string(envelope) {
        Ok(json) => json,
        Err(error) => {
            crate::route_fencing::cleanup_pending(state, tenant_id, request_id);
            let _ = cancel_route(state, fence).await;
            return Err(reject_peer(
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
        return Err(reject_peer(
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

async fn await_peer_response(
    state: Arc<GatewayState>,
    receiver: oneshot::Receiver<PeerRpcResponse>,
    fence: Option<crate::GatewayRequestFence>,
    request_id: String,
    timeout: Duration,
    mut telemetry: RouteObservation,
) -> PeerRpcResponse {
    tokio::select! {
        biased;
        _ = state.wait_for_shutdown() => {
            let _ = cancel_route(&state, &fence).await;
            reject_peer(&state, &mut telemetry, request_id, "gateway_shutting_down", RouteOutcome::Shutdown)
        }
        result = tokio::time::timeout(timeout, receiver) => match result {
            Ok(Ok(response)) => {
                if complete_route(&state, &fence, now_ms()).await.is_err() {
                    return reject_peer(&state, &mut telemetry, request_id, "registry_unavailable", RouteOutcome::TransportFailure);
                }
                state.metrics.message_routed();
                telemetry.finish(RouteOutcome::Success);
                response
            }
            Ok(Err(_)) => {
                let _ = cancel_route(&state, &fence).await;
                reject_peer(&state, &mut telemetry, request_id, "worker_connection_lost", RouteOutcome::TransportFailure)
            }
            Err(_) => {
                let _ = cancel_route(&state, &fence).await;
                reject_peer(&state, &mut telemetry, request_id, "worker_response_timeout", RouteOutcome::Timeout)
            }
        }
    }
}

fn reject_peer(
    state: &GatewayState,
    telemetry: &mut RouteObservation,
    request_id: impl Into<String>,
    error: impl Into<String>,
    outcome: RouteOutcome,
) -> PeerRpcResponse {
    state.metrics.routing_failure();
    telemetry.finish(outcome);
    PeerRpcResponse::rejected(request_id, error)
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
