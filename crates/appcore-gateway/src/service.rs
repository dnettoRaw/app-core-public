// =============================================================================
//        #######
//     ###       ###     F: service.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Axum HTTP and WebSocket routing service implementation.

use crate::authorization::{
    authenticate_connection, authenticate_mesh_request, client_connection_hash,
    worker_connection_hash,
};
use crate::config::{
    MAX_GATEWAY_CAPABILITIES, MAX_GATEWAY_HTTP_BODY_BYTES, MAX_GATEWAY_MESSAGE_BYTES,
};
use crate::mesh::{MeshPeerRequest, MESH_PEER_RELAY_PATH};
use crate::socket::{handle_client_socket, handle_worker_socket, WorkerSocketContext};
use crate::{EnvelopeRouter, GatewayState};
use appcore_contracts::InstallationId;
use appcore_security::RuntimeTokenClaims;
use appcore_types::{CapabilityName, ClusterId, CoreId, InstanceId, TenantId};
use axum::extract::{DefaultBodyLimit, Query, State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Connection parameters passed in the query string.
#[derive(Deserialize)]
pub struct ConnectionParams {
    pub(crate) tenant: Option<String>,
    pub(crate) cluster: Option<String>,
    pub(crate) installation: Option<String>,
    pub(crate) core: Option<String>,
    pub(crate) device: Option<String>,
    pub(crate) token: Option<String>,
    pub(crate) capabilities: Option<String>,
}

impl std::fmt::Debug for ConnectionParams {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ConnectionParams")
            .field("tenant", &self.tenant)
            .field("cluster", &self.cluster)
            .field("installation", &self.installation)
            .field("core", &self.core)
            .field("device", &self.device)
            .field("token", &self.token.as_ref().map(|_| "REDACTED"))
            .field("capabilities", &self.capabilities)
            .finish()
    }
}

/// Resolves a tenant from the deployment domain, with a query fallback for test helpers.
pub fn resolve_tenant(
    headers: &HeaderMap,
    params: &ConnectionParams,
    domain_suffix: &str,
) -> Option<TenantId> {
    resolve_host_tenant(headers, domain_suffix)
        .or_else(|| params.tenant.as_deref().and_then(valid_tenant))
}

/// Constructs the Axum router for the Gateway capability.
pub fn make_gateway_router(state: Arc<GatewayState>) -> Router {
    Router::new()
        .route("/v1/gateway/worker/connect", get(worker_connect_handler))
        .route("/v1/gateway/client/connect", get(client_connect_handler))
        .route(
            MESH_PEER_RELAY_PATH,
            axum::routing::post(mesh_peer_relay_handler),
        )
        .layer(DefaultBodyLimit::max(MAX_GATEWAY_HTTP_BODY_BYTES))
        .with_state(state)
}

async fn mesh_peer_relay_handler(
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    Json(request): Json<MeshPeerRequest>,
) -> Response {
    if state.is_shutting_down() {
        return (StatusCode::SERVICE_UNAVAILABLE, "Gateway is shutting down").into_response();
    }
    if request.validate_schema().is_err() {
        return (StatusCode::BAD_REQUEST, "Invalid mesh request").into_response();
    }
    if state.config().requires_authentication() {
        let Some(token) = extract_token(&headers) else {
            return (StatusCode::UNAUTHORIZED, "Missing credentials").into_response();
        };
        if authenticate_mesh_request(&state, token, &request, now_ms()).is_err() {
            return (StatusCode::FORBIDDEN, "Invalid credentials").into_response();
        }
    }
    let timeout = Duration::from_millis(request.timeout_ms);
    let response = EnvelopeRouter::route_mesh_request(state, request, timeout).await;
    (StatusCode::OK, Json(response)).into_response()
}

async fn worker_connect_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    Query(params): Query<ConnectionParams>,
) -> Response {
    if state.is_shutting_down() {
        return (StatusCode::SERVICE_UNAVAILABLE, "Gateway is shutting down").into_response();
    }
    let Some(tenant_id) = resolve_request_tenant(&state, &headers, &params) else {
        return (StatusCode::BAD_REQUEST, "Missing or invalid tenant").into_response();
    };
    let Some(cluster_id) = params.cluster.as_ref().and_then(valid_cluster) else {
        return (StatusCode::BAD_REQUEST, "Missing or invalid cluster").into_response();
    };
    let Some(installation_id) = params
        .installation
        .as_ref()
        .and_then(|value| InstallationId::new(value).ok())
    else {
        return (StatusCode::BAD_REQUEST, "Missing or invalid installation").into_response();
    };
    let Some(core_id) = params.core.as_ref().and_then(valid_core) else {
        return (StatusCode::BAD_REQUEST, "Missing or invalid core").into_response();
    };
    let capabilities = match parse_capabilities(params.capabilities.as_deref()) {
        Ok(capabilities) => capabilities,
        Err(message) => return (StatusCode::BAD_REQUEST, message).into_response(),
    };
    let expected_hash = worker_connection_hash(
        &tenant_id,
        &cluster_id,
        &installation_id,
        &core_id,
        &capabilities,
    );
    let claims = match authenticate_upgrade(&state, &headers, &params, &expected_hash) {
        Ok(claims) => claims,
        Err(error) => return error.into_response(),
    };
    ws.max_message_size(MAX_GATEWAY_MESSAGE_BYTES)
        .max_frame_size(MAX_GATEWAY_MESSAGE_BYTES)
        .on_upgrade(move |socket| {
            handle_worker_socket(
                state,
                WorkerSocketContext {
                    tenant_id,
                    cluster_id,
                    installation_id,
                    core_id,
                    capabilities,
                    expires_at_ms: claims.expires_at_ms,
                },
                socket,
            )
        })
}

async fn client_connect_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<GatewayState>>,
    headers: HeaderMap,
    Query(params): Query<ConnectionParams>,
) -> Response {
    if state.is_shutting_down() {
        return (StatusCode::SERVICE_UNAVAILABLE, "Gateway is shutting down").into_response();
    }
    let Some(tenant_id) = resolve_request_tenant(&state, &headers, &params) else {
        return (StatusCode::BAD_REQUEST, "Missing or invalid tenant").into_response();
    };
    let Some(cluster_id) = params.cluster.as_ref().and_then(valid_cluster) else {
        return (StatusCode::BAD_REQUEST, "Missing or invalid cluster").into_response();
    };
    let Some(device_id) = params.device.as_ref().and_then(valid_device) else {
        return (StatusCode::BAD_REQUEST, "Missing or invalid device").into_response();
    };
    let expected_hash = client_connection_hash(&tenant_id, &cluster_id, &device_id);
    let claims = match authenticate_upgrade(&state, &headers, &params, &expected_hash) {
        Ok(claims) => claims,
        Err(error) => return error.into_response(),
    };
    ws.max_message_size(MAX_GATEWAY_MESSAGE_BYTES)
        .max_frame_size(MAX_GATEWAY_MESSAGE_BYTES)
        .on_upgrade(move |socket| {
            handle_client_socket(state, tenant_id, cluster_id, claims, socket)
        })
}

fn authenticate_upgrade(
    state: &GatewayState,
    headers: &HeaderMap,
    params: &ConnectionParams,
    expected_hash: &str,
) -> Result<RuntimeTokenClaims, UpgradeError> {
    if params.token.is_some() {
        return Err(UpgradeError::QueryNotAllowed);
    }
    if !state.config().requires_authentication() {
        return Ok(insecure_claims());
    }
    let token = extract_token(headers).ok_or(UpgradeError::Missing)?;
    authenticate_connection(state, token, expected_hash, now_ms())
        .map_err(|_| UpgradeError::Invalid)
}

fn resolve_request_tenant(
    state: &GatewayState,
    headers: &HeaderMap,
    params: &ConnectionParams,
) -> Option<TenantId> {
    resolve_host_tenant(headers, &state.config().domain_suffix).or_else(|| {
        state
            .config()
            .bind_address
            .ip()
            .is_loopback()
            .then(|| params.tenant.as_deref().and_then(valid_tenant))
            .flatten()
    })
}

fn resolve_host_tenant(headers: &HeaderMap, domain_suffix: &str) -> Option<TenantId> {
    let host = headers.get("host")?.to_str().ok()?;
    let host = host.split(':').next().unwrap_or(host);
    let prefix = host.strip_suffix(domain_suffix)?.strip_suffix('.')?;
    valid_tenant(prefix)
}

fn parse_capabilities(value: Option<&str>) -> Result<Vec<CapabilityName>, &'static str> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value.split(',').collect::<Vec<_>>();
    if values.len() > MAX_GATEWAY_CAPABILITIES {
        return Err("Too many capabilities");
    }
    let mut unique = HashSet::new();
    let mut capabilities = Vec::with_capacity(values.len());
    for value in values {
        let capability = CapabilityName::new(value.trim()).map_err(|_| "Invalid capability")?;
        if unique.insert(capability.clone()) {
            capabilities.push(capability);
        }
    }
    Ok(capabilities)
}

enum UpgradeError {
    QueryNotAllowed,
    Missing,
    Invalid,
}

impl UpgradeError {
    fn into_response(self) -> Response {
        match self {
            Self::QueryNotAllowed => (
                StatusCode::BAD_REQUEST,
                "Query credentials are not accepted",
            )
                .into_response(),
            Self::Missing => (StatusCode::UNAUTHORIZED, "Missing credentials").into_response(),
            Self::Invalid => (StatusCode::FORBIDDEN, "Invalid credentials").into_response(),
        }
    }
}

fn extract_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get("authorization")?.to_str().ok()?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
}

fn valid_tenant(value: &str) -> Option<TenantId> {
    TenantId::new(value).ok()
}

fn valid_cluster(value: &String) -> Option<ClusterId> {
    ClusterId::new(value).ok()
}

fn valid_core(value: &String) -> Option<CoreId> {
    CoreId::new(value).ok()
}

fn valid_device(value: &String) -> Option<InstanceId> {
    InstanceId::new(value).ok()
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn insecure_claims() -> RuntimeTokenClaims {
    RuntimeTokenClaims {
        version: "v1".to_string(),
        purpose: "peer".to_string(),
        command_name: None,
        scope: None,
        subject: None,
        issued_at_ms: now_ms(),
        expires_at_ms: u64::MAX,
        jti: None,
        request_hash: None,
    }
}
