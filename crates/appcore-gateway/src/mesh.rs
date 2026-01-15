// =============================================================================
//        #######
//     ###       ###     F: mesh.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 10:16:57 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Mesh relay transport for Peer RPC over the Gateway.

use appcore_peer_rpc::{
    envelope_signing_hash, payload_hash, CancellationToken, PeerRpcEnvelope, PeerRpcError,
    PeerRpcHttpRequest, PeerRpcHttpResponse, PeerTransportProvider, PEER_COMMAND_PATH,
    PEER_HEALTH_PATH, PEER_MANIFEST_PATH, PEER_QUERY_PATH,
};
use appcore_transport::{
    send, HttpClientConfig, HttpHeader, HttpRequest, HttpTarget, TransportError,
};
use appcore_types::{CoreId, TenantId};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Stable schema marker for Gateway mesh HTTP relay messages.
pub const MESH_HTTP_SCHEMA_V1: &str = "appcore.gateway.mesh-http.v1";

/// Stable Gateway mesh relay endpoint.
pub const MESH_PEER_RELAY_PATH: &str = "/v1/gateway/mesh/peer";

/// Logical Peer RPC HTTP request forwarded through a Gateway worker socket.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshPeerRequest {
    /// Schema marker. Must be [`MESH_HTTP_SCHEMA_V1`].
    pub schema: String,
    /// Stable request identity.
    pub request_id: String,
    /// Tenant boundary for the target worker.
    pub target_tenant_id: TenantId,
    /// Target Core connected to the Gateway as a worker.
    pub target_core_id: CoreId,
    /// HTTP method from the logical Peer RPC request.
    pub method: String,
    /// Peer RPC path from the logical request.
    pub path: String,
    /// Encoded logical request body.
    pub body: Vec<u8>,
    /// Optional bearer credential forwarded to the target peer host.
    pub bearer_token: Option<String>,
    /// Per-attempt timeout in milliseconds.
    pub timeout_ms: u64,
    /// Maximum accepted response body bytes.
    pub max_response_bytes: usize,
}

impl std::fmt::Debug for MeshPeerRequest {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MeshPeerRequest")
            .field("schema", &self.schema)
            .field("request_id", &self.request_id)
            .field("target_tenant_id", &self.target_tenant_id)
            .field("target_core_id", &self.target_core_id)
            .field("method", &self.method)
            .field("path", &self.path)
            .field("body_bytes", &self.body.len())
            .field(
                "bearer_token",
                &self.bearer_token.as_ref().map(|_| "REDACTED"),
            )
            .field("timeout_ms", &self.timeout_ms)
            .field("max_response_bytes", &self.max_response_bytes)
            .finish()
    }
}

impl MeshPeerRequest {
    /// Builds a mesh relay request from a logical Peer RPC HTTP request.
    pub fn new(
        request_id: impl Into<String>,
        target_tenant_id: TenantId,
        target_core_id: CoreId,
        request: PeerRpcHttpRequest,
    ) -> Self {
        Self {
            schema: MESH_HTTP_SCHEMA_V1.to_string(),
            request_id: request_id.into(),
            target_tenant_id,
            target_core_id,
            method: request.method,
            path: request.path,
            body: request.body,
            bearer_token: request.bearer_token,
            timeout_ms: request.timeout_ms,
            max_response_bytes: request.max_response_bytes,
        }
    }

    /// Converts this relay request back to a logical Peer RPC HTTP request.
    pub fn into_peer_request(self) -> Result<PeerRpcHttpRequest, PeerRpcError> {
        self.validate_schema()?;
        Ok(PeerRpcHttpRequest {
            method: self.method,
            path: self.path,
            body: self.body,
            bearer_token: self.bearer_token,
            timeout_ms: self.timeout_ms,
            max_response_bytes: self.max_response_bytes,
        })
    }

    /// Validates the mesh relay schema marker.
    pub fn validate_schema(&self) -> Result<(), PeerRpcError> {
        if self.schema != MESH_HTTP_SCHEMA_V1 {
            return Err(PeerRpcError::ProtocolMismatch);
        }
        if self.request_id.is_empty()
            || self.body.len() > crate::config::MAX_GATEWAY_MESSAGE_BYTES
            || self.timeout_ms == 0
            || self.timeout_ms > crate::config::MAX_GATEWAY_REQUEST_TIMEOUT.as_millis() as u64
            || self.max_response_bytes == 0
            || self.max_response_bytes > crate::config::MAX_GATEWAY_MESSAGE_BYTES
        {
            return Err(PeerRpcError::PayloadTooLarge);
        }
        self.expected_request_hash().map(|_| ())
    }

    pub(crate) fn expected_request_hash(&self) -> Result<Option<String>, PeerRpcError> {
        match (self.method.as_str(), self.path.as_str()) {
            ("GET", PEER_HEALTH_PATH | PEER_MANIFEST_PATH) if self.body.is_empty() => Ok(None),
            ("POST", PEER_QUERY_PATH | PEER_COMMAND_PATH) => {
                let envelope = self.peer_envelope()?;
                Ok(Some(envelope_signing_hash(&envelope)))
            }
            _ => Err(PeerRpcError::InvalidEnvelope(
                "mesh_route_not_allowed".to_string(),
            )),
        }
    }

    pub(crate) fn peer_envelope(&self) -> Result<PeerRpcEnvelope, PeerRpcError> {
        let envelope = serde_json::from_slice::<PeerRpcEnvelope>(&self.body)
            .map_err(|_| PeerRpcError::InvalidEnvelope("mesh_peer_envelope_invalid".into()))?;
        if envelope.request_id != self.request_id
            || envelope.tenant_id != self.target_tenant_id
            || envelope.target_core_id != self.target_core_id
        {
            return Err(PeerRpcError::InvalidEnvelope(
                "mesh_routing_metadata_mismatch".to_string(),
            ));
        }
        if envelope.body_hash != payload_hash(&envelope.payload) {
            return Err(PeerRpcError::InvalidBodyHash);
        }
        Ok(envelope)
    }
}

/// Logical Peer RPC HTTP response returned through the Gateway mesh relay.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MeshPeerResponse {
    /// Schema marker. Must be [`MESH_HTTP_SCHEMA_V1`].
    pub schema: String,
    /// Stable request identity.
    pub request_id: String,
    /// HTTP status code returned by the target peer host.
    pub status_code: u16,
    /// Encoded response body returned by the target peer host.
    pub body: Vec<u8>,
    /// Controlled transport failure when the target worker could not complete the request.
    pub error: Option<String>,
}

impl std::fmt::Debug for MeshPeerResponse {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MeshPeerResponse")
            .field("schema", &self.schema)
            .field("request_id", &self.request_id)
            .field("status_code", &self.status_code)
            .field("body_bytes", &self.body.len())
            .field("has_error", &self.error.is_some())
            .finish()
    }
}

impl MeshPeerResponse {
    /// Creates a successful logical HTTP response.
    pub fn ok(request_id: impl Into<String>, response: PeerRpcHttpResponse) -> Self {
        Self {
            schema: MESH_HTTP_SCHEMA_V1.to_string(),
            request_id: request_id.into(),
            status_code: response.status_code,
            body: response.body,
            error: None,
        }
    }

    /// Creates a controlled relay failure.
    pub fn rejected(request_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            schema: MESH_HTTP_SCHEMA_V1.to_string(),
            request_id: request_id.into(),
            status_code: 503,
            body: Vec::new(),
            error: Some(error.into()),
        }
    }

    /// Converts this relay response to a logical Peer RPC HTTP response.
    pub fn into_peer_response(self) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        if self.schema != MESH_HTTP_SCHEMA_V1 {
            return Err(PeerRpcError::ProtocolMismatch);
        }
        if let Some(error) = self.error {
            return Err(PeerRpcError::Transport(error));
        }
        Ok(PeerRpcHttpResponse {
            status_code: self.status_code,
            body: self.body,
        })
    }

    pub(crate) fn validate_for_request(
        &self,
        request_id: &str,
        max_response_bytes: usize,
    ) -> Result<(), PeerRpcError> {
        if self.schema != MESH_HTTP_SCHEMA_V1 || self.request_id != request_id {
            return Err(PeerRpcError::InvalidResponse(
                "mesh response identity mismatch".to_string(),
            ));
        }
        if self.body.len() > max_response_bytes {
            return Err(PeerRpcError::PayloadTooLarge);
        }
        Ok(())
    }
}

/// Peer RPC transport that reaches Cores through a Gateway mesh relay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MeshPeerTransport {
    relay_url: String,
}

impl MeshPeerTransport {
    /// Creates a mesh transport using the deployment-selected Gateway relay URL.
    pub fn new(relay_url: impl Into<String>) -> Result<Self, PeerRpcError> {
        let relay_url = relay_url.into();
        HttpTarget::parse(&relay_url, MESH_PEER_RELAY_PATH).map_err(map_transport_error)?;
        Ok(Self { relay_url })
    }

    /// Returns the configured relay URL.
    pub fn relay_url(&self) -> &str {
        &self.relay_url
    }
}

impl PeerTransportProvider for MeshPeerTransport {
    fn send(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.send_request(base_url, request, None)
    }

    fn send_cancellable(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
        cancellation: &CancellationToken,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        self.send_request(base_url, request, Some(cancellation))
    }
}

impl MeshPeerTransport {
    fn send_request(
        &self,
        base_url: &str,
        request: PeerRpcHttpRequest,
        cancellation: Option<&CancellationToken>,
    ) -> Result<PeerRpcHttpResponse, PeerRpcError> {
        let target = MeshPeerTarget::parse(base_url)?;
        validate_peer_http_request(&request)?;
        let request_id = mesh_request_id(&request, &target);
        let relay_request =
            MeshPeerRequest::new(request_id, target.tenant_id, target.core_id, request);
        let max_response_bytes = relay_request.max_response_bytes;
        let body = serde_json::to_vec(&relay_request)
            .map_err(|error| PeerRpcError::Transport(error.to_string()))?;
        if body.len() > crate::config::MAX_GATEWAY_HTTP_BODY_BYTES {
            return Err(PeerRpcError::PayloadTooLarge);
        }
        let encoded_request_bytes = body.len();
        let mut http_request = HttpRequest::new("POST", body)
            .map_err(map_transport_error)?
            .with_header(
                HttpHeader::new("Content-Type", "application/json").map_err(map_transport_error)?,
            )
            .with_header(
                HttpHeader::new("Accept", "application/json").map_err(map_transport_error)?,
            );
        if let Some(token) = relay_request.bearer_token.as_ref() {
            http_request = http_request.with_header(
                HttpHeader::sensitive("Authorization", format!("Bearer {token}"))
                    .map_err(map_transport_error)?,
            );
        }
        let target = HttpTarget::parse(&self.relay_url, MESH_PEER_RELAY_PATH)
            .map_err(map_transport_error)?;
        let response = send(
            &target,
            &http_request,
            HttpClientConfig {
                timeout_ms: relay_request.timeout_ms.max(1),
                max_request_bytes: encoded_request_bytes,
                max_response_bytes: relay_request.max_response_bytes.saturating_add(65_536),
                max_header_bytes: 32_768,
            },
            cancellation,
        )
        .map_err(map_transport_error)?;
        if !(200..300).contains(&response.status_code) {
            return Err(PeerRpcError::EndpointUnavailable);
        }
        let response = serde_json::from_slice::<MeshPeerResponse>(&response.body)
            .map_err(|error| PeerRpcError::InvalidResponse(error.to_string()))?
            .into_peer_response()?;
        if response.body.len() > max_response_bytes {
            return Err(PeerRpcError::PayloadTooLarge);
        }
        Ok(response)
    }
}

fn validate_peer_http_request(request: &PeerRpcHttpRequest) -> Result<(), PeerRpcError> {
    if request.timeout_ms == 0
        || request.timeout_ms > crate::config::MAX_GATEWAY_REQUEST_TIMEOUT.as_millis() as u64
        || request.body.len() > crate::config::MAX_GATEWAY_MESSAGE_BYTES
        || request.max_response_bytes == 0
        || request.max_response_bytes > crate::config::MAX_GATEWAY_MESSAGE_BYTES
    {
        return Err(PeerRpcError::PayloadTooLarge);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MeshPeerTarget {
    tenant_id: TenantId,
    core_id: CoreId,
}

impl MeshPeerTarget {
    fn parse(base_url: &str) -> Result<Self, PeerRpcError> {
        let value = base_url
            .strip_prefix("mesh://")
            .or_else(|| base_url.strip_prefix("appcore-mesh://"))
            .ok_or_else(|| PeerRpcError::Transport("invalid mesh peer URL".to_string()))?;
        let (tenant, core) = value
            .split_once('/')
            .ok_or_else(|| PeerRpcError::Transport("invalid mesh peer URL".to_string()))?;
        Ok(Self {
            tenant_id: TenantId::new(tenant)
                .map_err(|error| PeerRpcError::Transport(format!("{error:?}")))?,
            core_id: CoreId::new(core)
                .map_err(|error| PeerRpcError::Transport(format!("{error:?}")))?,
        })
    }
}

fn mesh_request_id(request: &PeerRpcHttpRequest, target: &MeshPeerTarget) -> String {
    if let Ok(envelope) = serde_json::from_slice::<appcore_peer_rpc::PeerRpcEnvelope>(&request.body)
    {
        return envelope.request_id;
    }
    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local mesh request collisions
    static MESH_REQUEST_COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter = MESH_REQUEST_COUNTER.fetch_add(1, Ordering::Relaxed);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    format!(
        "mesh-{}-{}-{}-{}-{}",
        target.tenant_id.as_str(),
        target.core_id.as_str(),
        now_ms,
        std::process::id(),
        counter
    )
}

fn map_transport_error(error: TransportError) -> PeerRpcError {
    match error {
        TransportError::ResponseTooLarge { .. } | TransportError::RequestTooLarge { .. } => {
            PeerRpcError::PayloadTooLarge
        }
        TransportError::Timeout
        | TransportError::ConnectionRefused
        | TransportError::Dns(_)
        | TransportError::Cancelled => PeerRpcError::EndpointUnavailable,
        other => PeerRpcError::Transport(other.to_string()),
    }
}
