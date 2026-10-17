// =============================================================================
//        #######
//     ###       ###     F: lease_service.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 08:42:56 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.6
// =============================================================================

//! Opt-in service arbitration HTTP boundary. Does not route application writes.
use crate::{
    GatewayState, ServiceLeaseOperationV1 as Op, ServiceLeaseRequestV1, GATEWAY_SERVICE_LEASE_PATH,
    MAX_SERVICE_LEASE_REQUEST_BYTES,
};
use appcore_distributed_contracts::control_plane::{
    lease_authority::ServiceLeaseAuthority,
    v1::{ControlPlaneError, ControlPlaneResult, ServiceLeaderLease},
};
use appcore_security::{CommandTokenValidator, TokenClaims};
use axum::{
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use std::{
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};
use tokio::sync::Semaphore;

/// Isolated token domain for service authority; relay/HA credentials do not authorize it.
pub fn service_lease_token_claims() -> TokenClaims {
    TokenClaims {
        issuer: "appcore".into(),
        audience: "gateway-service-lease-v1".into(),
        salt: "gateway-service-lease-v1".into(),
        ttl_ms: 30_000,
    }
}
#[derive(Clone)]
struct LeaseState {
    gateway: Arc<GatewayState>,
    authority: Arc<dyn ServiceLeaseAuthority>,
    slots: Arc<Semaphore>,
    replay: Arc<dyn appcore_peer_rpc::PeerNonceStore>,
}
/// Explicitly compose a server-clock authority. Authentication is always required,
/// including when Gateway insecure-testing is enabled. Eight operations maximum,
/// no waiting admission queue. Provider blocking I/O runs outside async worker threads.
/// Supply a dedicated durable replay store shared across restarts/replicas; no default is inferred.
/// Deployment owns certified I/O bounds, issuer ACLs, TLS and draining before shutdown.
pub fn make_service_lease_router(
    gateway: Arc<GatewayState>,
    authority: Arc<dyn ServiceLeaseAuthority>,
    replay: Arc<dyn appcore_peer_rpc::PeerNonceStore>,
) -> Router {
    Router::new()
        .route(GATEWAY_SERVICE_LEASE_PATH, post(handle))
        .layer(DefaultBodyLimit::max(MAX_SERVICE_LEASE_REQUEST_BYTES))
        .with_state(LeaseState {
            gateway,
            authority,
            replay,
            slots: Arc::new(Semaphore::new(8)),
        })
}
async fn handle(
    State(state): State<LeaseState>,
    headers: HeaderMap,
    Json(request): Json<ServiceLeaseRequestV1>,
) -> Response {
    if state.gateway.is_shutting_down() {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    }
    if request.version != 1 || request.identity.protocol_version.as_u16() != 1 {
        return (StatusCode::BAD_REQUEST, "NO MORE SUPPORTED PLEASE UPDATE").into_response();
    }
    if request.validate().is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    let Some(token) = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
    else {
        return StatusCode::UNAUTHORIZED.into_response();
    };
    let Ok(slot) = state.slots.clone().try_acquire_owned() else {
        return StatusCode::SERVICE_UNAVAILABLE.into_response();
    };
    let token = token.to_owned();
    let worker = tokio::task::spawn_blocking(move || {
        let _slot = slot;
        let now = server_now();
        let expiry = authorize(&state.gateway, &*state.replay, &token, &request, now);
        if expiry.is_err() || expiry.is_ok_and(|expiry| expiry <= server_now()) {
            state.gateway.metrics.authentication_failure();
            return Err(StatusCode::FORBIDDEN);
        }
        tokio::runtime::Handle::current()
            .block_on(dispatch(&*state.authority, request))
            .map_err(|error| match error {
                ControlPlaneError::Conflict(_) | ControlPlaneError::LeaseUnavailable => {
                    StatusCode::CONFLICT
                }
                ControlPlaneError::Rejected(_) => StatusCode::BAD_REQUEST,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            })
    });
    match worker.await {
        Ok(Ok(lease)) => Json(lease).into_response(),
        Ok(Err(status)) => status.into_response(),
        Err(_) => StatusCode::SERVICE_UNAVAILABLE.into_response(),
    }
}

pub(crate) fn authorize(
    gateway: &GatewayState,
    replay: &dyn appcore_peer_rpc::PeerNonceStore,
    token: &str,
    request: &ServiceLeaseRequestV1,
    now: u64,
) -> Result<u64, ()> {
    let hash = request.request_hash().map_err(|_| ())?;
    let claims = CommandTokenValidator::new(&gateway.token_provider, service_lease_token_claims())
        .validate_and_get_claims(
            token,
            request.token_purpose(),
            Some(request.token_name()),
            now,
            Some(&hash),
        )
        .map_err(|_| ())?;
    if claims.request_hash.as_deref() != Some(hash.as_str())
        || claims.subject.as_deref() != Some(request.identity.core_id.as_str())
        || claims.command_name.as_deref() != Some(request.token_name())
        || claims.scope.is_some()
    {
        return Err(());
    }
    let expiry_ceiling = match &request.action {
        Op::Acquire { ttl_ms } => claims.issued_at_ms.checked_add(*ttl_ms).ok_or(())?,
        Op::Renew { lease, .. } | Op::Release { lease } | Op::Check { lease } => {
            lease.expires_at_ms
        }
        Op::Inspect => claims.expires_at_ms,
    };
    if claims.expires_at_ms > expiry_ceiling {
        return Err(());
    }
    let jti = claims.jti.as_deref().ok_or(())?;
    if jti.is_empty() || jti.len() > 96 {
        return Err(());
    }
    replay
        .check_and_record(&format!("lease-v1:{jti}"), claims.expires_at_ms, now)
        .map_err(|_| ())?;
    Ok(claims.expires_at_ms)
}
fn server_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(u64::MAX, |d| d.as_millis().try_into().unwrap_or(u64::MAX))
}
async fn dispatch(
    authority: &dyn ServiceLeaseAuthority,
    request: ServiceLeaseRequestV1,
) -> ControlPlaneResult<Option<ServiceLeaderLease>> {
    match request.action {
        Op::Acquire { ttl_ms } => authority
            .acquire(&request.identity, &request.service_id, ttl_ms)
            .await
            .map(Some),
        Op::Renew { lease, ttl_ms } => authority.renew(&lease, ttl_ms).await.map(Some),
        Op::Release { lease } => authority.release(&lease).await.map(|()| None),
        Op::Inspect => {
            authority
                .inspect(&request.identity, &request.service_id)
                .await
        }
        Op::Check { lease } => authority.check(&lease).await.map(|()| Some(lease)),
    }
}
