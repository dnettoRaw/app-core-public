// =============================================================================
//        #######
//     ###       ###     F: chunk_transfer.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/25 00:00:00 by dnettoRaw
// =============================================================================

//! Generic bounded range transfer independent of update or storage crates.

use sha2::{Digest, Sha256};
use std::fmt::{Debug, Formatter};
use std::io::{Read, Seek, SeekFrom};

/// Default maximum object size accepted by the range-transfer adapter.
pub const DEFAULT_MAX_TRANSFER_OBJECT_BYTES: u64 = 64 * 1024 * 1024;
/// Default maximum range size accepted by the range-transfer adapter.
pub const DEFAULT_MAX_TRANSFER_CHUNK_BYTES: u32 = 64 * 1024;

/// Bounds applied before a range is allocated or read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PeerRpcChunkTransferPolicy {
    /// Maximum complete object size.
    pub max_object_bytes: u64,
    /// Maximum bytes returned for one range.
    pub max_chunk_bytes: u32,
}

impl Default for PeerRpcChunkTransferPolicy {
    fn default() -> Self {
        Self {
            max_object_bytes: DEFAULT_MAX_TRANSFER_OBJECT_BYTES,
            max_chunk_bytes: DEFAULT_MAX_TRANSFER_CHUNK_BYTES,
        }
    }
}

impl PeerRpcChunkTransferPolicy {
    /// Validates non-zero bounded transfer policy.
    pub fn validate(self) -> Result<Self, PeerRpcChunkTransferError> {
        if self.max_object_bytes == 0
            || self.max_chunk_bytes == 0
            || u64::from(self.max_chunk_bytes) > self.max_object_bytes
        {
            return Err(PeerRpcChunkTransferError::InvalidPolicy);
        }
        Ok(self)
    }
}

/// Typed request for one resumable object range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRpcChunkTransferRequestV2 {
    /// SHA-256 identity of the complete object, lowercase hexadecimal.
    pub object_hash: String,
    /// Byte offset from the beginning of the object.
    pub offset: u64,
    /// Requested range length.
    pub length: u32,
    /// Declared complete object size.
    pub total_bytes: u64,
}

impl PeerRpcChunkTransferRequestV2 {
    /// Creates a request after validating identity and range arithmetic.
    pub fn new(
        object_hash: impl Into<String>,
        offset: u64,
        length: u32,
        total_bytes: u64,
    ) -> Result<Self, PeerRpcChunkTransferError> {
        let request = Self {
            object_hash: object_hash.into(),
            offset,
            length,
            total_bytes,
        };
        request.validate(PeerRpcChunkTransferPolicy::default())?;
        Ok(request)
    }

    /// Validates identity, bounds and non-overflowing range arithmetic.
    pub fn validate(
        &self,
        policy: PeerRpcChunkTransferPolicy,
    ) -> Result<(), PeerRpcChunkTransferError> {
        policy.validate()?;
        validate_hash(&self.object_hash)?;
        if self.total_bytes > policy.max_object_bytes {
            return Err(PeerRpcChunkTransferError::ObjectTooLarge);
        }
        if self.length == 0 || self.length > policy.max_chunk_bytes {
            return Err(PeerRpcChunkTransferError::ChunkTooLarge);
        }
        let end = self
            .offset
            .checked_add(u64::from(self.length))
            .ok_or(PeerRpcChunkTransferError::OffsetOutOfRange)?;
        if end > self.total_bytes {
            return Err(PeerRpcChunkTransferError::OffsetOutOfRange);
        }
        Ok(())
    }
}

/// Typed response for one object range. Payload debug output is redacted.
#[derive(Clone, PartialEq, Eq)]
pub struct PeerRpcChunkTransferResponseV2 {
    /// SHA-256 identity of the complete object.
    pub object_hash: String,
    /// Byte offset of `payload` in the complete object.
    pub offset: u64,
    /// Declared complete object size.
    pub total_bytes: u64,
    /// Exact bytes returned for this range.
    pub payload: Vec<u8>,
    /// SHA-256 digest of `payload`.
    pub chunk_hash: String,
}

impl Debug for PeerRpcChunkTransferResponseV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcChunkTransferResponseV2")
            .field("object_hash", &self.object_hash)
            .field("offset", &self.offset)
            .field("total_bytes", &self.total_bytes)
            .field("payload_bytes", &self.payload.len())
            .field("chunk_hash", &self.chunk_hash)
            .finish()
    }
}

/// Failures from bounded range serving or response verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PeerRpcChunkTransferError {
    /// Policy contains a zero or inconsistent bound.
    #[error("peer chunk transfer policy is invalid")]
    InvalidPolicy,
    /// Object identity is not a lowercase SHA-256 hexadecimal value.
    #[error("peer chunk transfer object hash is invalid")]
    InvalidObjectHash,
    /// Complete object exceeds the configured bound.
    #[error("peer chunk transfer object is too large")]
    ObjectTooLarge,
    /// Requested range exceeds the configured chunk bound.
    #[error("peer chunk transfer chunk is too large")]
    ChunkTooLarge,
    /// Requested range is outside the declared object.
    #[error("peer chunk transfer offset is out of range")]
    OffsetOutOfRange,
    /// Source seek or read failed.
    #[error("peer chunk transfer source failed")]
    Source,
    /// Source ended before the exact range was read.
    #[error("peer chunk transfer range is incomplete")]
    Incomplete,
    /// Response object identity does not match the requested object.
    #[error("peer chunk transfer object identity mismatched")]
    ObjectMismatch,
    /// Response range metadata does not match the request.
    #[error("peer chunk transfer response range mismatched")]
    RangeMismatch,
    /// Response bytes do not match their declared digest.
    #[error("peer chunk transfer chunk digest mismatched")]
    DigestMismatch,
}

/// Reads exactly one bounded range from a seekable source.
pub fn serve_chunk<R: Read + Seek>(
    source: &mut R,
    expected_object_hash: &str,
    request: &PeerRpcChunkTransferRequestV2,
    policy: PeerRpcChunkTransferPolicy,
) -> Result<PeerRpcChunkTransferResponseV2, PeerRpcChunkTransferError> {
    request.validate(policy)?;
    validate_hash(expected_object_hash)?;
    if request.object_hash != expected_object_hash {
        return Err(PeerRpcChunkTransferError::ObjectMismatch);
    }
    let source_bytes = source
        .seek(SeekFrom::End(0))
        .map_err(|_| PeerRpcChunkTransferError::Source)?;
    if source_bytes != request.total_bytes {
        return Err(PeerRpcChunkTransferError::RangeMismatch);
    }
    source
        .seek(SeekFrom::Start(request.offset))
        .map_err(|_| PeerRpcChunkTransferError::Source)?;
    let mut payload = vec![0; request.length as usize];
    source
        .read_exact(&mut payload)
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::UnexpectedEof => PeerRpcChunkTransferError::Incomplete,
            _ => PeerRpcChunkTransferError::Source,
        })?;
    Ok(PeerRpcChunkTransferResponseV2 {
        object_hash: request.object_hash.clone(),
        offset: request.offset,
        total_bytes: request.total_bytes,
        chunk_hash: digest(&payload),
        payload,
    })
}

/// Verifies a received range before it is committed to a resumable sink.
pub fn verify_chunk(
    request: &PeerRpcChunkTransferRequestV2,
    response: PeerRpcChunkTransferResponseV2,
    policy: PeerRpcChunkTransferPolicy,
) -> Result<Vec<u8>, PeerRpcChunkTransferError> {
    request.validate(policy)?;
    if response.object_hash != request.object_hash {
        return Err(PeerRpcChunkTransferError::ObjectMismatch);
    }
    if response.offset != request.offset
        || response.total_bytes != request.total_bytes
        || response.payload.len() != request.length as usize
    {
        return Err(PeerRpcChunkTransferError::RangeMismatch);
    }
    if response.chunk_hash != digest(&response.payload) {
        return Err(PeerRpcChunkTransferError::DigestMismatch);
    }
    Ok(response.payload)
}

fn validate_hash(value: &str) -> Result<(), PeerRpcChunkTransferError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(PeerRpcChunkTransferError::InvalidObjectHash);
    }
    Ok(())
}

fn digest(payload: &[u8]) -> String {
    Sha256::digest(payload)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    const HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    #[test]
    fn serves_and_verifies_resumable_range_without_full_object_copy() {
        let mut source = Cursor::new(b"0123456789abcdef".to_vec());
        let request = PeerRpcChunkTransferRequestV2::new(HASH, 4, 4, 16).unwrap();
        let response = serve_chunk(
            &mut source,
            HASH,
            &request,
            PeerRpcChunkTransferPolicy::default(),
        )
        .unwrap();
        assert_eq!(
            verify_chunk(&request, response, PeerRpcChunkTransferPolicy::default()),
            Ok(b"4567".to_vec())
        );
    }

    #[test]
    fn rejects_oversized_and_out_of_range_requests_before_reading() {
        let policy = PeerRpcChunkTransferPolicy {
            max_object_bytes: 16,
            max_chunk_bytes: 4,
        };
        let oversized = PeerRpcChunkTransferRequestV2 {
            object_hash: HASH.to_string(),
            offset: 0,
            length: 5,
            total_bytes: 16,
        };
        assert_eq!(
            oversized.validate(policy),
            Err(PeerRpcChunkTransferError::ChunkTooLarge)
        );
        let out_of_range = PeerRpcChunkTransferRequestV2 {
            object_hash: HASH.to_string(),
            offset: 15,
            length: 2,
            total_bytes: 16,
        };
        assert_eq!(
            out_of_range.validate(policy),
            Err(PeerRpcChunkTransferError::OffsetOutOfRange)
        );
    }

    #[test]
    fn rejects_tampered_chunk_digest_and_object_identity() {
        let request = PeerRpcChunkTransferRequestV2::new(HASH, 0, 4, 4).unwrap();
        let response = PeerRpcChunkTransferResponseV2 {
            object_hash: HASH.to_string(),
            offset: 0,
            total_bytes: 4,
            payload: b"test".to_vec(),
            chunk_hash: "bad".to_string(),
        };
        assert_eq!(
            verify_chunk(&request, response, PeerRpcChunkTransferPolicy::default()),
            Err(PeerRpcChunkTransferError::DigestMismatch)
        );
    }
}
