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

use crate::v2::{PeerRpcStreamErrorV2, PeerRpcStreamFrameV2};

/// Serializes one V2 frame and returns SHA-256 over those exact JSON body bytes.
pub fn stream_frame_signing_hash(
    frame: &PeerRpcStreamFrameV2,
) -> Result<String, PeerRpcStreamErrorV2> {
    let encoded = serde_json::to_vec(frame).map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
    Ok(crate::payload_hash(&encoded))
}
