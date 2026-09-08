// =============================================================================
//        #######
//     ###       ###     F: stream_signing.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Canonical authentication binding for one complete V2 frame.

use crate::v2::{
    PeerRpcStreamCodecV2, PeerRpcStreamErrorV2, PeerRpcStreamFrameV2,
    MAX_PEER_RPC_BINARY_FRAME_BYTES_V2,
};

/// Serializes one V2 frame and returns SHA-256 over those exact JSON body bytes.
pub fn stream_frame_signing_hash(
    frame: &PeerRpcStreamFrameV2,
) -> Result<String, PeerRpcStreamErrorV2> {
    json_signing_hash(frame)
}

/// Returns SHA-256 over the exact body emitted by the selected V2 codec.
pub fn stream_frame_signing_hash_with_codec(
    frame: &PeerRpcStreamFrameV2,
    codec: PeerRpcStreamCodecV2,
) -> Result<String, PeerRpcStreamErrorV2> {
    match codec {
        PeerRpcStreamCodecV2::Json => json_signing_hash(frame),
        PeerRpcStreamCodecV2::Binary => {
            let encoded =
                crate::v2::encode_binary_frame_v2(frame, MAX_PEER_RPC_BINARY_FRAME_BYTES_V2)
                    .map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
            Ok(crate::payload_hash(&encoded))
        }
    }
}

fn json_signing_hash(frame: &PeerRpcStreamFrameV2) -> Result<String, PeerRpcStreamErrorV2> {
    crate::json_payload_hash(frame).map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)
}

#[cfg(test)]
mod tests {
    use super::stream_frame_signing_hash;
    use crate::v2::{
        PeerRpcChunkEncodingV2, PeerRpcStreamChunkV2, PeerRpcStreamFrameV2,
        PEER_RPC_PROTOCOL_VERSION_V2,
    };
    use appcore_core::ProtocolVersion;

    #[test]
    fn incremental_json_hash_matches_the_exact_large_frame_body() {
        let payload = vec![0x5a; 64 * 1024];
        let chunk_hash = crate::payload_hash(&payload);
        let frame = PeerRpcStreamFrameV2::Chunk(PeerRpcStreamChunkV2 {
            protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
            request_id: "request-large-frame".to_string(),
            stream_id: "stream-large-frame".to_string(),
            sequence: 7,
            encoding: PeerRpcChunkEncodingV2::Identity,
            payload,
            decoded_bytes: 64 * 1024,
            chunk_hash,
        });
        let encoded = serde_json::to_vec(&frame).unwrap();

        assert_eq!(
            stream_frame_signing_hash(&frame).unwrap(),
            crate::payload_hash(&encoded)
        );
    }
}
