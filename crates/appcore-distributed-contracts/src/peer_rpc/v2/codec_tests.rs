// =============================================================================
//        #######
//     ###       ###     F: codec_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================
// appcore-norm: test

use super::*;
use crate::peer_rpc::v1::PeerRpcCallKind;
use appcore_types::{CapabilityName, ClusterId, CoreId, ProtocolVersion, TenantId};
use sha2::{Digest, Sha256};

fn chunk_frame(payload: Vec<u8>) -> PeerRpcStreamFrameV2 {
    PeerRpcStreamFrameV2::Chunk(PeerRpcStreamChunkV2 {
        protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
        request_id: "request-codec-1".to_string(),
        stream_id: "stream-codec-1".to_string(),
        sequence: 7,
        encoding: PeerRpcChunkEncodingV2::Identity,
        decoded_bytes: payload.len() as u32,
        chunk_hash: format!("{:x}", Sha256::digest(&payload)),
        payload,
    })
}

fn open_frame() -> PeerRpcStreamFrameV2 {
    PeerRpcStreamFrameV2::Open(Box::new(PeerRpcStreamOpenV2 {
        protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
        request_id: "request-codec-1".to_string(),
        stream_id: "stream-codec-1".to_string(),
        trace_id: "trace-codec-1".to_string(),
        direction: PeerRpcStreamDirectionV2::Request,
        call_kind: PeerRpcCallKind::Query,
        source_core_id: CoreId::new("core-a").unwrap(),
        target_core_id: CoreId::new("core-b").unwrap(),
        tenant_id: TenantId::new("tenant-a").unwrap(),
        cluster_id: ClusterId::new("cluster-a").unwrap(),
        timestamp_ms: 1_000,
        deadline_ms: 2_000,
        nonce: "nonce-codec-1".to_string(),
        capability: CapabilityName::new("runtime.query").unwrap(),
        payload_bytes: 0,
        chunk_bytes: 64 * 1024,
        chunk_count: 0,
        idempotency_key: None,
        trace: None,
    }))
}

#[test]
fn binary_frame_and_nested_reply_round_trip_exact_bytes() {
    let payload: Vec<u8> = (0_u8..=u8::MAX).cycle().take(64 * 1024).collect();
    let frame = chunk_frame(payload);
    let encoded = encode_binary_frame_v2(&frame, MAX_PEER_RPC_BINARY_FRAME_BYTES_V2).unwrap();
    assert_eq!(
        decode_binary_frame_v2(&encoded, MAX_PEER_RPC_BINARY_FRAME_BYTES_V2).unwrap(),
        frame
    );

    let reply = PeerRpcStreamReplyV2 {
        request_id: "request-codec-1".to_string(),
        stream_id: "stream-codec-1".to_string(),
        next_sequence: 8,
        received_bytes: 64 * 1024,
        response_frame: Some(Box::new(frame)),
        complete: false,
    };
    let encoded = encode_binary_reply_v2(&reply, MAX_PEER_RPC_BINARY_FRAME_BYTES_V2).unwrap();
    assert_eq!(
        decode_binary_reply_v2(&encoded, MAX_PEER_RPC_BINARY_FRAME_BYTES_V2).unwrap(),
        reply
    );
}

#[test]
fn binary_codec_reduces_incompressible_chunk_wire_bytes() {
    let payload: Vec<u8> = (0_u8..=u8::MAX).cycle().take(64 * 1024).collect();
    let frame = chunk_frame(payload);
    let json = serde_json::to_vec(&frame).unwrap();
    let binary = encode_binary_frame_v2(&frame, MAX_PEER_RPC_BINARY_FRAME_BYTES_V2).unwrap();
    assert!(binary.len() * 100 < json.len() * 80);
    let json_value: serde_json::Value = serde_json::from_slice(&json).unwrap();
    assert!(json_value["frame"]["payload"].is_string());
}

#[test]
fn binary_codec_rejects_limit_marker_version_kind_and_length_failures() {
    let frame = open_frame();
    assert_eq!(
        encode_binary_frame_v2(&frame, 0),
        Err(PeerRpcBinaryCodecErrorV2::InvalidLimit)
    );
    assert_eq!(
        encode_binary_frame_v2(&frame, 8),
        Err(PeerRpcBinaryCodecErrorV2::FrameTooLarge)
    );
    let encoded = encode_binary_frame_v2(&frame, 4_096).unwrap();
    assert_eq!(
        decode_binary_reply_v2(&encoded, 4_096),
        Err(PeerRpcBinaryCodecErrorV2::InvalidMessageKind)
    );
    let mut invalid = encoded.clone();
    invalid[0] ^= 1;
    assert_eq!(
        decode_binary_frame_v2(&invalid, 4_096),
        Err(PeerRpcBinaryCodecErrorV2::InvalidMagic)
    );
    invalid = encoded.clone();
    invalid[8] = PEER_RPC_BINARY_CODEC_VERSION_V2.saturating_add(1);
    assert_eq!(
        decode_binary_frame_v2(&invalid, 4_096),
        Err(PeerRpcBinaryCodecErrorV2::UnsupportedVersion)
    );
    invalid = encoded;
    invalid.push(0);
    assert_eq!(
        decode_binary_frame_v2(&invalid, 4_096),
        Err(PeerRpcBinaryCodecErrorV2::InvalidLength)
    );
}
