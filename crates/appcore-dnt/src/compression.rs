// =============================================================================
//        #######
//     ###       ###     F: compression.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 10:29:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:07:11 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Optional payload compaction for DNT envelopes.

use crate::flags::{validate_flags, DNT_FLAG_PAYLOAD_DEFLATE};
use crate::{DntError, DntResult};
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use std::io::{Read, Write};
use zeroize::Zeroize;

/// Payload storage transform recorded in the authenticated DNT header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DntCompression {
    /// Store the codec output directly.
    None,
    /// Store the codec output through zlib-wrapped DEFLATE at balanced compression.
    Deflate,
}

impl DntCompression {
    pub(crate) fn from_flags(flags: u32) -> DntResult<Self> {
        validate_flags(flags)?;
        if flags & DNT_FLAG_PAYLOAD_DEFLATE != 0 {
            return Ok(Self::Deflate);
        }
        Ok(Self::None)
    }

    /// Returns true when the payload is compacted before encryption.
    pub fn is_compacted(self) -> bool {
        self != Self::None
    }
}

pub(crate) fn encode_payload(
    flags: u32,
    mut encoded_payload: Vec<u8>,
    max_payload_bytes: Option<u64>,
) -> DntResult<Vec<u8>> {
    match DntCompression::from_flags(flags)? {
        DntCompression::None => Ok(encoded_payload),
        DntCompression::Deflate => {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            let write_result = encoder.write_all(&encoded_payload);
            encoded_payload.zeroize();
            write_result.map_err(|_| DntError::CodecFailed)?;
            let compacted = encoder.finish().map_err(|_| DntError::CodecFailed)?;
            enforce_max(compacted.len() as u64, max_payload_bytes)?;
            Ok(compacted)
        }
    }
}

pub(crate) fn decode_payload(
    flags: u32,
    stored_payload: &[u8],
    max_payload_bytes: Option<u64>,
) -> DntResult<Vec<u8>> {
    match DntCompression::from_flags(flags)? {
        DntCompression::None => Ok(stored_payload.to_vec()),
        DntCompression::Deflate => inflate_bounded(stored_payload, max_payload_bytes),
    }
}

fn inflate_bounded(input: &[u8], max_payload_bytes: Option<u64>) -> DntResult<Vec<u8>> {
    let Some(max_payload_bytes) = max_payload_bytes else {
        return Err(DntError::PayloadTooLarge);
    };
    let read_limit = max_payload_bytes.saturating_add(1);
    let mut decoder = ZlibDecoder::new(input).take(read_limit);
    let mut output = Vec::new();
    if decoder.read_to_end(&mut output).is_err() {
        output.zeroize();
        return Err(DntError::CodecFailed);
    }
    if let Err(error) = enforce_max(output.len() as u64, Some(max_payload_bytes)) {
        output.zeroize();
        return Err(error);
    }
    Ok(output)
}

fn enforce_max(actual: u64, max: Option<u64>) -> DntResult<()> {
    if max.is_some_and(|max| actual > max) {
        return Err(DntError::PayloadTooLarge);
    }
    Ok(())
}
