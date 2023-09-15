// =============================================================================
//        #######
//     ###       ###     F: plaintext.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 10:49:08 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT encrypted plaintext layout helpers.

use crate::{DntError, DntHeader, DntResult};
use std::ops::Range;

pub(crate) fn encode_plaintext(metadata: &[u8], payload: &[u8]) -> DntResult<Vec<u8>> {
    let metadata_len = u32::try_from(metadata.len()).map_err(|_| DntError::PayloadTooLarge)?;
    let capacity = 4usize
        .checked_add(metadata.len())
        .and_then(|length| length.checked_add(payload.len()))
        .ok_or(DntError::PayloadTooLarge)?;
    let mut output = Vec::with_capacity(capacity);
    output.extend_from_slice(&metadata_len.to_be_bytes());
    output.extend_from_slice(metadata);
    output.extend_from_slice(payload);
    Ok(output)
}

pub(crate) fn decode_plaintext_ranges(
    plaintext: &[u8],
    header: &DntHeader,
) -> DntResult<(Range<usize>, usize)> {
    let prefix = plaintext.get(..4).ok_or(DntError::InvalidFormat)?;
    let metadata_len = u32::from_be_bytes([prefix[0], prefix[1], prefix[2], prefix[3]]);
    if metadata_len != header.encrypted_metadata_length {
        return Err(DntError::AuthenticationFailed);
    }
    let metadata_len = metadata_len as usize;
    let metadata_start = 4usize;
    let payload_start = metadata_start
        .checked_add(metadata_len)
        .ok_or(DntError::InvalidFormat)?;
    let metadata = plaintext
        .get(metadata_start..payload_start)
        .ok_or(DntError::InvalidFormat)?;
    let payload = plaintext
        .get(payload_start..)
        .ok_or(DntError::InvalidFormat)?;
    if payload.len() as u64 != header.payload_length {
        return Err(DntError::AuthenticationFailed);
    }
    Ok((
        metadata_start..metadata_start + metadata.len(),
        payload_start,
    ))
}
