// =============================================================================
//        #######
//     ###       ###     F: stream_registry_protocol.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Response frame construction for the V2 stream registry.

use super::*;
use crate::stream_registry_types::PeerRpcStreamRegistryConfig;
use crate::v2::{
    PeerRpcStreamDirectionV2, PeerRpcStreamErrorV2, PeerRpcStreamFrameV2, PeerRpcStreamOpenV2,
    PeerRpcStreamReplyV2, PEER_RPC_PROTOCOL_VERSION_V2,
};

pub(crate) fn response_open(
    request: &PeerRpcStreamOpenV2,
    payload_bytes: u64,
    config: &PeerRpcStreamRegistryConfig,
    now_ms: u64,
) -> Result<PeerRpcStreamOpenV2, PeerRpcStreamErrorV2> {
    if now_ms >= request.deadline_ms {
        return Err(PeerRpcStreamErrorV2::Expired);
    }
    if payload_bytes > config.chunk_limits.max_payload_bytes {
        return Err(PeerRpcStreamErrorV2::PayloadTooLarge);
    }
    let chunk_bytes = config.chunk_limits.max_chunk_bytes.min(u32::MAX as usize) as u32;
    let chunk_count = if payload_bytes == 0 {
        0
    } else {
        u32::try_from((payload_bytes - 1) / u64::from(chunk_bytes) + 1)
            .map_err(|_| PeerRpcStreamErrorV2::PayloadTooLarge)?
    };
    Ok(PeerRpcStreamOpenV2 {
        protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
        request_id: request.request_id.clone(),
        stream_id: next_response_stream_id(),
        trace_id: request.trace_id.clone(),
        direction: PeerRpcStreamDirectionV2::Response,
        call_kind: request.call_kind,
        source_core_id: request.target_core_id.clone(),
        target_core_id: request.source_core_id.clone(),
        tenant_id: request.tenant_id.clone(),
        cluster_id: request.cluster_id.clone(),
        timestamp_ms: now_ms,
        deadline_ms: request.deadline_ms,
        nonce: next_response_stream_id(),
        capability: request.capability.clone(),
        payload_bytes,
        chunk_bytes,
        chunk_count,
        idempotency_key: None,
        trace: request.trace.clone(),
    })
}

pub(crate) fn reply(
    request_id: &str,
    stream_id: &str,
    next_sequence: u32,
    received_bytes: u64,
    response_frame: Option<Box<PeerRpcStreamFrameV2>>,
    complete: bool,
) -> PeerRpcStreamReplyV2 {
    PeerRpcStreamReplyV2 {
        request_id: request_id.to_string(),
        stream_id: stream_id.to_string(),
        next_sequence,
        received_bytes,
        response_frame,
        complete,
    }
}

fn next_response_stream_id() -> String {
    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local stream identifier collisions
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("response-{}-{sequence}", std::process::id())
}
