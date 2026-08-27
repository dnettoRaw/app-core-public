// =============================================================================
//        #######
//     ###       ###     F: host.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;
use crate::client::now_ms;
use crate::stream_host::{
    peer_v2_binary_command_handler, peer_v2_binary_query_handler, peer_v2_command_handler,
    peer_v2_query_handler,
};
use crate::transport::{gzip_decode_limited, gzip_if_beneficial};
use crate::v2::{
    PEER_COMMAND_BINARY_PATH_V2, PEER_COMMAND_PATH_V2, PEER_QUERY_BINARY_PATH_V2,
    PEER_QUERY_PATH_V2,
};
use axum::extract::{DefaultBodyLimit, Extension};

/// Shared immutable state used by the peer RPC HTTP router.
#[derive(Clone)]
pub struct PeerRpcHttpState {
    pub(crate) manifest: DistributedCoreManifest,
    pub(crate) validator: PeerRpcValidator,
    pub(crate) dispatcher: Arc<dyn PeerRpcDispatcher>,
    pub(crate) authenticator: Arc<dyn PeerRpcAuthenticator>,
}

/// HTTP host exposing the stable peer health, manifest, query, and command routes.
pub struct PeerRpcHttpHost {
    host: String,
    port: u16,
    state: PeerRpcHttpState,
    v2_registry: Option<Arc<PeerRpcStreamRegistry>>,
    v2_binary_codec: bool,
}
impl PeerRpcHttpHost {
    /// Creates a peer HTTP host with explicit validation, dispatch, and authentication.
    pub fn new(
        host: impl Into<String>,
        port: u16,
        manifest: DistributedCoreManifest,
        validator: PeerRpcValidator,
        dispatcher: Arc<dyn PeerRpcDispatcher>,
        authenticator: Arc<dyn PeerRpcAuthenticator>,
    ) -> Self {
        let state = PeerRpcHttpState {
            manifest,
            validator,
            dispatcher,
            authenticator,
        };
        Self {
            host: host.into(),
            port,
            state,
            v2_registry: None,
            v2_binary_codec: false,
        }
    }

    /// Explicitly enables signed V2 query and command frame routes.
    pub fn with_v2_stream_registry(mut self, registry: Arc<PeerRpcStreamRegistry>) -> Self {
        self.v2_registry = Some(registry);
        self
    }

    /// Enables the distinct binary V2 routes when a V2 registry is installed.
    ///
    /// JSON remains the default. This method never changes or redirects V1 or
    /// JSON V2 routes.
    pub fn with_v2_binary_codec(mut self) -> Self {
        self.v2_binary_codec = true;
        self
    }

    /// Returns the configured Axum router.
    pub fn router(&self) -> Router {
        let mut router = Router::new()
            .route(PEER_HEALTH_PATH, get(peer_health_handler))
            .route(PEER_MANIFEST_PATH, get(peer_manifest_handler))
            .route(PEER_QUERY_PATH, post(peer_query_handler))
            .route(PEER_COMMAND_PATH, post(peer_command_handler));
        if let Some(registry) = &self.v2_registry {
            let v2_routes = Router::new()
                .route(PEER_QUERY_PATH_V2, post(peer_v2_query_handler))
                .route(PEER_COMMAND_PATH_V2, post(peer_v2_command_handler))
                .layer(DefaultBodyLimit::max(registry.max_http_frame_bytes()))
                .layer(Extension(Arc::clone(registry)));
            router = router.merge(v2_routes);
            if self.v2_binary_codec {
                let binary_routes = Router::new()
                    .route(
                        PEER_QUERY_BINARY_PATH_V2,
                        post(peer_v2_binary_query_handler),
                    )
                    .route(
                        PEER_COMMAND_BINARY_PATH_V2,
                        post(peer_v2_binary_command_handler),
                    )
                    .layer(DefaultBodyLimit::max(registry.max_http_frame_bytes()))
                    .layer(Extension(Arc::clone(registry)));
                router = router.merge(binary_routes);
            }
        }
        router.with_state(self.state.clone())
    }

    /// Runs the host until the shared shutdown flag is set.
    pub fn run_until_shutdown(&self, shutdown: Arc<AtomicBool>) -> io::Result<()> {
        let address = format!("{}:{}", self.host, self.port);
        let router = self.router();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(io::Error::other)?;
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::bind(address).await?;
            axum::serve(listener, router)
                .with_graceful_shutdown(wait_for_shutdown(shutdown))
                .await
        })
    }
}

async fn peer_health_handler(
    State(state): State<PeerRpcHttpState>,
    headers: HeaderMap,
) -> Response {
    match state
        .authenticator
        .authenticate(bearer_token(&headers), None, now_ms())
    {
        Ok(()) => {
            let identity = &state.manifest.identity;
            (
                StatusCode::OK,
                Json(PeerHealthResponse {
                    ok: true,
                    core_id: identity.core_id.clone(),
                    tenant_id: identity.tenant_id.clone(),
                    cluster_id: identity.cluster_id.clone(),
                }),
            )
                .into_response()
        }
        Err(error) => peer_error_response("health", error),
    }
}

async fn peer_manifest_handler(
    State(state): State<PeerRpcHttpState>,
    headers: HeaderMap,
) -> Response {
    match state
        .authenticator
        .authenticate(bearer_token(&headers), None, now_ms())
    {
        Ok(()) => (
            StatusCode::OK,
            Json(PeerManifestResponse {
                advertisement: crate::advertisement::advertisement_from_manifest(&state.manifest),
            }),
        )
            .into_response(),
        Err(error) => peer_error_response("manifest", error),
    }
}

async fn peer_query_handler(
    State(state): State<PeerRpcHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let envelope = match decode_peer_envelope(&state, &headers, &body) {
        Ok(envelope) => envelope,
        Err(error) => return peer_error_response("invalid", error),
    };
    handle_peer_envelope(state, headers, envelope, PeerRpcKind::Query).await
}

async fn peer_command_handler(
    State(state): State<PeerRpcHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    let envelope = match decode_peer_envelope(&state, &headers, &body) {
        Ok(envelope) => envelope,
        Err(error) => return peer_error_response("invalid", error),
    };
    handle_peer_envelope(state, headers, envelope, PeerRpcKind::Command).await
}

pub(crate) fn decode_peer_envelope(
    state: &PeerRpcHttpState,
    headers: &HeaderMap,
    body: &[u8],
) -> Result<PeerRpcEnvelope, PeerRpcError> {
    let max_bytes = state.validator.max_envelope_bytes();
    let decoded = match headers.get(header::CONTENT_ENCODING) {
        None => {
            if body.len() > max_bytes {
                return Err(PeerRpcError::PayloadTooLarge);
            }
            body.to_vec()
        }
        Some(value) if value.as_bytes().eq_ignore_ascii_case(b"gzip") => {
            gzip_decode_limited(body, max_bytes)?
        }
        Some(_) => {
            return Err(PeerRpcError::InvalidEnvelope(
                "unsupported_content_encoding".to_string(),
            ))
        }
    };
    serde_json::from_slice(&decoded)
        .map_err(|error| PeerRpcError::InvalidEnvelope(error.to_string()))
}

#[derive(Debug, Clone, Copy)]
enum PeerRpcKind {
    Query,
    Command,
}

async fn handle_peer_envelope(
    state: PeerRpcHttpState,
    headers: HeaderMap,
    envelope: PeerRpcEnvelope,
    kind: PeerRpcKind,
) -> Response {
    let accepts_gzip = accepts_gzip(&headers);
    let request_id = envelope.request_id.clone();
    let now = now_ms();
    if let Err(error) = state.authenticator.authenticate(
        bearer_token(&headers),
        Some(&envelope_signing_hash(&envelope)),
        now,
    ) {
        return peer_error_response(&request_id, error);
    }
    if let Err(error) = state.validator.validate(&envelope, now) {
        return peer_error_response(&request_id, error);
    }
    let dispatcher = Arc::clone(&state.dispatcher);
    let response = tokio::task::spawn_blocking(move || match kind {
        PeerRpcKind::Query => dispatcher.dispatch_peer_query(envelope),
        PeerRpcKind::Command => dispatcher.dispatch_peer_command(envelope),
    })
    .await
    .unwrap_or_else(|_| {
        Err(PeerRpcError::Transport(
            "peer dispatcher panicked".to_string(),
        ))
    });
    match response {
        Ok(response) => peer_json_response(StatusCode::OK, &response, accepts_gzip),
        Err(error) => peer_error_response(&request_id, error),
    }
}

fn peer_json_response<T>(status: StatusCode, value: &T, allow_gzip: bool) -> Response
where
    T: Serialize,
{
    if allow_gzip {
        if let Ok(body) = serde_json::to_vec(value) {
            if let Ok(Some(compressed)) = gzip_if_beneficial(&body) {
                let mut response = (status, compressed).into_response();
                response.headers_mut().insert(
                    header::CONTENT_TYPE,
                    HeaderValue::from_static("application/json"),
                );
                response
                    .headers_mut()
                    .insert(header::CONTENT_ENCODING, HeaderValue::from_static("gzip"));
                return response;
            }
        }
    }
    (status, Json(value)).into_response()
}

pub(crate) fn accepts_gzip(headers: &HeaderMap) -> bool {
    headers
        .get(header::ACCEPT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value.split(',').any(|encoding| {
                let mut parts = encoding.trim().split(';');
                let name = parts.next().unwrap_or_default().trim();
                let disabled = parts.any(|parameter| {
                    parameter
                        .trim()
                        .strip_prefix("q=")
                        .map(|quality| quality.trim() == "0" || quality.trim() == "0.0")
                        .unwrap_or(false)
                });
                name.eq_ignore_ascii_case("gzip") && !disabled
            })
        })
        .unwrap_or(false)
}

fn peer_error_response(request_id: &str, error: PeerRpcError) -> Response {
    let status = match error {
        PeerRpcError::Unauthorized => StatusCode::UNAUTHORIZED,
        PeerRpcError::Forbidden => StatusCode::FORBIDDEN,
        PeerRpcError::EndpointUnavailable => StatusCode::SERVICE_UNAVAILABLE,
        PeerRpcError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
        PeerRpcError::TenantMismatch
        | PeerRpcError::ClusterMismatch
        | PeerRpcError::TargetMismatch
        | PeerRpcError::Expired
        | PeerRpcError::NonceReplay
        | PeerRpcError::InvalidBodyHash
        | PeerRpcError::InvalidResponse(_)
        | PeerRpcError::Transport(_)
        | PeerRpcError::InvalidEnvelope(_)
        | PeerRpcError::RemoteRejected(_) => StatusCode::BAD_REQUEST,
        PeerRpcError::ProtocolMismatch => StatusCode::CONFLICT,
        PeerRpcError::NonceCacheFull => StatusCode::SERVICE_UNAVAILABLE,
    };
    (
        status,
        Json(PeerRpcResponse::rejected(
            request_id,
            peer_error_code(&error),
        )),
    )
        .into_response()
}

fn peer_error_code(error: &PeerRpcError) -> &'static str {
    match error {
        PeerRpcError::PayloadTooLarge => "payload_too_large",
        PeerRpcError::Unauthorized => "unauthorized",
        PeerRpcError::Forbidden => "forbidden",
        PeerRpcError::EndpointUnavailable => "endpoint_unavailable",
        PeerRpcError::TenantMismatch => "tenant_mismatch",
        PeerRpcError::ClusterMismatch => "cluster_mismatch",
        PeerRpcError::TargetMismatch => "target_mismatch",
        PeerRpcError::ProtocolMismatch => "protocol_mismatch",
        PeerRpcError::Expired => "expired",
        PeerRpcError::NonceReplay => "nonce_replay",
        PeerRpcError::NonceCacheFull => "nonce_cache_full",
        PeerRpcError::InvalidBodyHash => "invalid_body_hash",
        PeerRpcError::InvalidResponse(_) => "invalid_response",
        PeerRpcError::Transport(_) => "transport",
        PeerRpcError::InvalidEnvelope(_) => "invalid_envelope",
        PeerRpcError::RemoteRejected(_) => "invalid_response",
    }
}

pub(crate) fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
}

async fn wait_for_shutdown(shutdown: Arc<AtomicBool>) {
    loop {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
