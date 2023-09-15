// =============================================================================
//        #######
//     ###       ###     F: codec.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 10:29:16 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT codec contracts.

use crate::{CodecError, CodecId};
use zeroize::Zeroize;

/// Payload codec used before encryption and after authentication.
pub trait DntCodec: Send + Sync {
    /// Returns the stable codec identifier.
    fn codec_id(&self) -> CodecId;
    /// Returns true when this codec can decode the authenticated identifier.
    fn matches_codec_id(&self, codec_id: &CodecId) -> bool {
        self.codec_id() == *codec_id
    }
    /// Encodes caller-owned bytes before sealing.
    fn encode(&self, value: &[u8]) -> Result<Vec<u8>, CodecError>;
    /// Decodes authenticated plaintext bytes after opening.
    fn decode(&self, payload: &[u8]) -> Result<Vec<u8>, CodecError>;
    /// Decodes an owned authenticated payload after opening.
    ///
    /// Codecs that are identity transforms can return `payload` directly to
    /// avoid an allocation in the read path. Transforming codecs may keep the
    /// default implementation.
    fn decode_owned(&self, mut payload: Vec<u8>) -> Result<Vec<u8>, CodecError> {
        let decoded = self.decode(&payload);
        payload.zeroize();
        decoded
    }
}

/// Identity codec for arbitrary binary payloads.
#[derive(Debug, Clone, Copy, Default)]
pub struct BytesCodec;

impl DntCodec for BytesCodec {
    fn codec_id(&self) -> CodecId {
        // appcore-norm: allow(clippy::expect_used) reason: codec identifier is a validated package constant
        CodecId::new("bytes").expect("static codec id")
    }

    fn matches_codec_id(&self, codec_id: &CodecId) -> bool {
        codec_id.as_str() == "bytes"
    }

    fn encode(&self, value: &[u8]) -> Result<Vec<u8>, CodecError> {
        Ok(value.to_vec())
    }

    fn decode(&self, payload: &[u8]) -> Result<Vec<u8>, CodecError> {
        Ok(payload.to_vec())
    }

    fn decode_owned(&self, payload: Vec<u8>) -> Result<Vec<u8>, CodecError> {
        Ok(payload)
    }
}

/// Identity codec for caller-validated JSON bytes.
#[derive(Debug, Clone, Copy, Default)]
pub struct IdentityJsonCodec;

impl DntCodec for IdentityJsonCodec {
    fn codec_id(&self) -> CodecId {
        // appcore-norm: allow(clippy::expect_used) reason: codec identifier is a validated package constant
        CodecId::new("json").expect("static codec id")
    }

    fn matches_codec_id(&self, codec_id: &CodecId) -> bool {
        codec_id.as_str() == "json"
    }

    fn encode(&self, value: &[u8]) -> Result<Vec<u8>, CodecError> {
        Ok(value.to_vec())
    }

    fn decode(&self, payload: &[u8]) -> Result<Vec<u8>, CodecError> {
        Ok(payload.to_vec())
    }

    fn decode_owned(&self, payload: Vec<u8>) -> Result<Vec<u8>, CodecError> {
        Ok(payload)
    }
}
