// =============================================================================
//        #######
//     ###       ###     F: codec.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/03 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/03 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Bounded transport-neutral Peer RPC V1 JSON decoding.

use crate::{PeerRpcEnvelope, PeerRpcError};

/// Decodes one V1 JSON envelope after enforcing its encoded byte limit.
///
/// The input is borrowed directly. Hosts do not need to copy an uncompressed
/// request body before Serde decodes its owned envelope fields.
pub fn decode_peer_rpc_envelope_json(
    input: &[u8],
    max_encoded_bytes: usize,
) -> Result<PeerRpcEnvelope, PeerRpcError> {
    if input.len() > max_encoded_bytes {
        return Err(PeerRpcError::PayloadTooLarge);
    }
    serde_json::from_slice(input).map_err(|error| PeerRpcError::InvalidEnvelope(error.to_string()))
}
