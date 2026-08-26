// =============================================================================
//        #######
//     ###       ###     F: stream_host.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Authenticated HTTP exchange for explicitly enabled V2 frame routes.

use super::*;
use crate::client::now_ms;
use crate::host::bearer_token;
use crate::v2::{
    PeerRpcStreamErrorV2, PeerRpcStreamFrameV2, PeerRpcStreamHttpErrorCodeV2,
    PeerRpcStreamHttpErrorV2,
};
use axum::extract::{Extension, State};

pub(crate) async fn peer_v2_query_handler(
    State(state): State<PeerRpcHttpState>,
    Extension(registry): Extension<Arc<PeerRpcStreamRegistry>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    handle_v2_frame(state, registry, headers, body, PeerRpcCallKind::Query).await
}

pub(crate) async fn peer_v2_command_handler(
    State(state): State<PeerRpcHttpState>,
    Extension(registry): Extension<Arc<PeerRpcStreamRegistry>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    handle_v2_frame(state, registry, headers, body, PeerRpcCallKind::Command).await
}

async fn handle_v2_frame(
    state: PeerRpcHttpState,
    registry: Arc<PeerRpcStreamRegistry>,
    headers: HeaderMap,
    body: Bytes,
    expected_kind: PeerRpcCallKind,
) -> Response {
    if headers.contains_key(header::CONTENT_ENCODING)
        || body.len() > registry.max_http_frame_bytes()
    {
        return stream_error_response(None, None, PeerRpcStreamErrorV2::PayloadTooLarge);
    }
    let frame = match serde_json::from_slice::<PeerRpcStreamFrameV2>(&body) {
        Ok(frame) => frame,
        Err(_) => return stream_error_response(None, None, PeerRpcStreamErrorV2::InvalidConfig),
    };
    let request_id = frame.request_id().to_string();
    let stream_id = frame.stream_id().to_string();
    let signing_hash = payload_hash(&body);
    if let Err(error) =
        state
            .authenticator
            .authenticate(bearer_token(&headers), Some(&signing_hash), now_ms())
    {
        return authentication_error_response(Some(request_id), Some(stream_id), error);
    }
    let now = now_ms();
    if let PeerRpcStreamFrameV2::Open(open) = &frame {
        if let Err(error) = state.validator.validate_stream_open_v2(open, now) {
            return validation_error_response(Some(request_id), Some(stream_id), error);
        }
    }
    let result = tokio::task::spawn_blocking(move || registry.exchange(expected_kind, frame, now))
        .await
        .unwrap_or(Err(PeerRpcStreamErrorV2::Io));
    match result {
        Ok(reply) => (StatusCode::OK, Json(reply)).into_response(),
        Err(error) => stream_error_response(Some(request_id), Some(stream_id), error),
    }
}

fn validation_error_response(
    request_id: Option<String>,
    stream_id: Option<String>,
    error: PeerRpcError,
) -> Response {
    use PeerRpcStreamHttpErrorCodeV2 as Code;
    let (status, code) = match error {
        PeerRpcError::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, Code::PayloadTooLarge),
        PeerRpcError::TenantMismatch => (StatusCode::CONFLICT, Code::TenantMismatch),
        PeerRpcError::ClusterMismatch => (StatusCode::CONFLICT, Code::ClusterMismatch),
        PeerRpcError::TargetMismatch => (StatusCode::CONFLICT, Code::TargetMismatch),
        PeerRpcError::ProtocolMismatch => (StatusCode::CONFLICT, Code::ProtocolMismatch),
        PeerRpcError::Expired => (StatusCode::REQUEST_TIMEOUT, Code::Expired),
        PeerRpcError::NonceReplay => (StatusCode::CONFLICT, Code::NonceReplay),
        PeerRpcError::NonceCacheFull => (StatusCode::SERVICE_UNAVAILABLE, Code::CapacityExceeded),
        _ => (StatusCode::BAD_REQUEST, Code::InvalidFrame),
    };
    stream_http_error(status, request_id, stream_id, code)
}

fn authentication_error_response(
    request_id: Option<String>,
    stream_id: Option<String>,
    error: PeerRpcError,
) -> Response {
    let (status, code) = match error {
        PeerRpcError::Unauthorized => (
            StatusCode::UNAUTHORIZED,
            PeerRpcStreamHttpErrorCodeV2::Unauthorized,
        ),
        _ => (
            StatusCode::FORBIDDEN,
            PeerRpcStreamHttpErrorCodeV2::Forbidden,
        ),
    };
    stream_http_error(status, request_id, stream_id, code)
}

fn stream_error_response(
    request_id: Option<String>,
    stream_id: Option<String>,
    error: PeerRpcStreamErrorV2,
) -> Response {
    let (status, code) = stream_error_status_code(error);
    stream_http_error(status, request_id, stream_id, code)
}

fn stream_http_error(
    status: StatusCode,
    request_id: Option<String>,
    stream_id: Option<String>,
    code: PeerRpcStreamHttpErrorCodeV2,
) -> Response {
    (
        status,
        Json(PeerRpcStreamHttpErrorV2 {
            request_id,
            stream_id,
            code,
        }),
    )
        .into_response()
}

fn stream_error_status_code(
    error: PeerRpcStreamErrorV2,
) -> (StatusCode, PeerRpcStreamHttpErrorCodeV2) {
    use PeerRpcStreamErrorV2 as Error;
    use PeerRpcStreamHttpErrorCodeV2 as Code;
    match error {
        Error::InvalidConfig => (StatusCode::BAD_REQUEST, Code::InvalidFrame),
        Error::ProtocolMismatch => (StatusCode::CONFLICT, Code::ProtocolMismatch),
        Error::PayloadTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, Code::PayloadTooLarge),
        Error::ChunkTooLarge => (StatusCode::PAYLOAD_TOO_LARGE, Code::ChunkTooLarge),
        Error::InvalidSequence => (StatusCode::CONFLICT, Code::InvalidSequence),
        Error::InvalidChunkLength => (StatusCode::BAD_REQUEST, Code::InvalidChunkLength),
        Error::InvalidChunkHash => (StatusCode::BAD_REQUEST, Code::InvalidChunkHash),
        Error::InvalidPayloadHash => (StatusCode::BAD_REQUEST, Code::InvalidPayloadHash),
        Error::IdentityMismatch => (StatusCode::CONFLICT, Code::IdentityMismatch),
        Error::DirectionMismatch => (StatusCode::CONFLICT, Code::DirectionMismatch),
        Error::CallKindMismatch => (StatusCode::CONFLICT, Code::CallKindMismatch),
        Error::Incomplete => (StatusCode::CONFLICT, Code::Incomplete),
        Error::Expired => (StatusCode::REQUEST_TIMEOUT, Code::Expired),
        Error::Cancelled => (StatusCode::CONFLICT, Code::Cancelled),
        Error::Io => (StatusCode::INTERNAL_SERVER_ERROR, Code::Io),
        Error::InvalidEncoding => (StatusCode::BAD_REQUEST, Code::InvalidEncoding),
        Error::Closed => (StatusCode::GONE, Code::Closed),
        Error::CapacityExceeded => (StatusCode::SERVICE_UNAVAILABLE, Code::CapacityExceeded),
    }
}
