//! Bounded, resumable payload chunks for sync adapters.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::error::{SyncError, SyncResult};

/// Maximum bytes carried by one resumable chunk.
pub const MAX_SYNC_CHUNK_BYTES: usize = 64 * 1024;
/// Maximum bytes accepted for one chunked transfer.
pub const MAX_SYNC_CHUNK_TOTAL_BYTES: usize = 16 * 1024 * 1024;
/// Maximum chunks retained by one assembler.
pub const MAX_SYNC_CHUNKS: usize = 4_096;
/// Maximum transfer identifier length.
pub const MAX_SYNC_TRANSFER_ID_BYTES: usize = 128;

/// A bounded portion of an opaque sync payload.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncPayloadChunk {
    /// Stable identifier for the logical transfer.
    pub transfer_id: String,
    /// Byte offset within the complete payload.
    pub offset: u64,
    /// Total payload length in bytes.
    pub total_bytes: u64,
    /// Lowercase SHA-256 digest of the complete payload.
    pub payload_sha256: String,
    /// Lowercase SHA-256 digest of this chunk.
    pub chunk_sha256: String,
    /// Opaque payload bytes.
    pub payload: Vec<u8>,
}

impl std::fmt::Debug for SyncPayloadChunk {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SyncPayloadChunk")
            .field("transfer_id", &self.transfer_id)
            .field("offset", &self.offset)
            .field("total_bytes", &self.total_bytes)
            .field("payload_sha256", &self.payload_sha256)
            .field("chunk_sha256", &self.chunk_sha256)
            .field("payload_bytes", &self.payload.len())
            .finish()
    }
}

impl SyncPayloadChunk {
    /// Validates bounds, hashes and the chunk range.
    pub fn validate(&self) -> SyncResult<()> {
        validate_transfer_id(&self.transfer_id)?;
        validate_hash(&self.payload_sha256, "payload_sha256")?;
        validate_hash(&self.chunk_sha256, "chunk_sha256")?;
        if self.total_bytes == 0 || self.total_bytes > MAX_SYNC_CHUNK_TOTAL_BYTES as u64 {
            return Err(SyncError::InvalidChunk("invalid total_bytes".to_owned()));
        }
        if self.payload.is_empty() || self.payload.len() > MAX_SYNC_CHUNK_BYTES {
            return Err(SyncError::InvalidChunk("invalid chunk size".to_owned()));
        }
        let end = self
            .offset
            .checked_add(self.payload.len() as u64)
            .ok_or_else(|| SyncError::InvalidChunk("chunk range overflow".to_owned()))?;
        if end > self.total_bytes {
            return Err(SyncError::InvalidChunk(
                "chunk exceeds total_bytes".to_owned(),
            ));
        }
        if sha256_hex(&self.payload) != self.chunk_sha256 {
            return Err(SyncError::InvalidChunk("chunk digest mismatch".to_owned()));
        }
        Ok(())
    }
}

/// A missing byte range in a resumable transfer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncChunkRange {
    /// First missing byte offset.
    pub offset: u64,
    /// Number of missing bytes.
    pub len: u64,
}

/// Payload-free progress information for a resumable transfer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncChunkProgress {
    /// Stable identifier for the logical transfer.
    pub transfer_id: String,
    /// Total payload length in bytes.
    pub total_bytes: u64,
    /// Lowercase SHA-256 digest of the complete payload.
    pub payload_sha256: String,
    /// Number of unique bytes retained by the assembler.
    pub received_bytes: u64,
    /// Missing byte ranges, in ascending order.
    pub missing: Vec<SyncChunkRange>,
}

/// Result of inserting a chunk into an assembler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncChunkInsert {
    /// New bytes were accepted.
    Accepted,
    /// The same chunk was already accepted and was safely replayed.
    Duplicate,
}

/// Bounded assembler for out-of-order and resumable sync chunks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncChunkAssembler {
    transfer_id: String,
    total_bytes: u64,
    payload_sha256: String,
    received_bytes: u64,
    chunks: BTreeMap<u64, Vec<u8>>,
}

impl SyncChunkAssembler {
    /// Creates an empty assembler for one logical transfer.
    pub fn new(
        transfer_id: impl Into<String>,
        total_bytes: u64,
        payload_sha256: impl Into<String>,
    ) -> SyncResult<Self> {
        let transfer_id = transfer_id.into();
        let payload_sha256 = payload_sha256.into();
        validate_transfer_id(&transfer_id)?;
        validate_hash(&payload_sha256, "payload_sha256")?;
        if total_bytes == 0 || total_bytes > MAX_SYNC_CHUNK_TOTAL_BYTES as u64 {
            return Err(SyncError::InvalidChunk("invalid total_bytes".to_owned()));
        }
        Ok(Self {
            transfer_id,
            total_bytes,
            payload_sha256,
            received_bytes: 0,
            chunks: BTreeMap::new(),
        })
    }

    /// Inserts one chunk, accepting an identical replay idempotently.
    pub fn insert(&mut self, chunk: SyncPayloadChunk) -> SyncResult<SyncChunkInsert> {
        chunk.validate()?;
        if chunk.transfer_id != self.transfer_id
            || chunk.total_bytes != self.total_bytes
            || chunk.payload_sha256 != self.payload_sha256
        {
            return Err(SyncError::InvalidChunk(
                "transfer metadata mismatch".to_owned(),
            ));
        }
        if let Some(existing) = self.chunks.get(&chunk.offset) {
            return if existing == &chunk.payload {
                Ok(SyncChunkInsert::Duplicate)
            } else {
                Err(SyncError::ChunkConflict(chunk.offset))
            };
        }
        if self.chunks.len() >= MAX_SYNC_CHUNKS {
            return Err(SyncError::InvalidChunk("too many chunks".to_owned()));
        }
        let end = chunk.offset + chunk.payload.len() as u64;
        if self.overlaps_existing(chunk.offset, end) {
            return Err(SyncError::ChunkConflict(chunk.offset));
        }
        self.received_bytes += chunk.payload.len() as u64;
        self.chunks.insert(chunk.offset, chunk.payload);
        Ok(SyncChunkInsert::Accepted)
    }

    /// Returns bounded, payload-free progress and missing ranges.
    pub fn progress(&self) -> SyncChunkProgress {
        SyncChunkProgress {
            transfer_id: self.transfer_id.clone(),
            total_bytes: self.total_bytes,
            payload_sha256: self.payload_sha256.clone(),
            received_bytes: self.received_bytes,
            missing: self.missing_ranges(),
        }
    }

    /// Assembles the payload only after every byte is present and verified.
    pub fn assemble(&self) -> SyncResult<Vec<u8>> {
        if self.received_bytes != self.total_bytes || !self.missing_ranges().is_empty() {
            return Err(SyncError::ChunkIncomplete);
        }
        let mut payload = Vec::with_capacity(self.total_bytes as usize);
        for bytes in self.chunks.values() {
            payload.extend_from_slice(bytes);
        }
        if sha256_hex(&payload) != self.payload_sha256 {
            return Err(SyncError::InvalidChunk(
                "payload digest mismatch".to_owned(),
            ));
        }
        Ok(payload)
    }

    fn overlaps_existing(&self, offset: u64, end: u64) -> bool {
        self.chunks.iter().any(|(existing_offset, bytes)| {
            let existing_end = *existing_offset + bytes.len() as u64;
            offset < existing_end && *existing_offset < end
        })
    }

    fn missing_ranges(&self) -> Vec<SyncChunkRange> {
        let mut missing = Vec::new();
        let mut cursor = 0;
        for (offset, bytes) in &self.chunks {
            if cursor < *offset {
                missing.push(SyncChunkRange {
                    offset: cursor,
                    len: *offset - cursor,
                });
            }
            cursor = (*offset + bytes.len() as u64).max(cursor);
        }
        if cursor < self.total_bytes {
            missing.push(SyncChunkRange {
                offset: cursor,
                len: self.total_bytes - cursor,
            });
        }
        missing
    }
}

/// Splits one opaque payload into bounded chunks with resumable metadata.
pub fn split_sync_payload(
    transfer_id: impl Into<String>,
    payload: &[u8],
    chunk_bytes: usize,
) -> SyncResult<Vec<SyncPayloadChunk>> {
    let transfer_id = transfer_id.into();
    validate_transfer_id(&transfer_id)?;
    if payload.is_empty() || payload.len() > MAX_SYNC_CHUNK_TOTAL_BYTES {
        return Err(SyncError::InvalidChunk("invalid payload size".to_owned()));
    }
    if chunk_bytes == 0 || chunk_bytes > MAX_SYNC_CHUNK_BYTES {
        return Err(SyncError::InvalidChunk("invalid chunk_bytes".to_owned()));
    }
    let payload_sha256 = sha256_hex(payload);
    payload
        .chunks(chunk_bytes)
        .enumerate()
        .map(|(index, bytes)| SyncPayloadChunk {
            transfer_id: transfer_id.clone(),
            offset: (index * chunk_bytes) as u64,
            total_bytes: payload.len() as u64,
            payload_sha256: payload_sha256.clone(),
            chunk_sha256: sha256_hex(bytes),
            payload: bytes.to_vec(),
        })
        .collect::<Vec<_>>()
        .into_iter()
        .map(|chunk| {
            chunk.validate()?;
            Ok(chunk)
        })
        .collect()
}

fn validate_transfer_id(transfer_id: &str) -> SyncResult<()> {
    if transfer_id.is_empty()
        || transfer_id.len() > MAX_SYNC_TRANSFER_ID_BYTES
        || transfer_id.chars().any(char::is_control)
    {
        return Err(SyncError::InvalidChunk("invalid transfer_id".to_owned()));
    }
    Ok(())
}

fn validate_hash(value: &str, name: &str) -> SyncResult<()> {
    if value.len() != 64
        || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        || value != value.to_ascii_lowercase()
    {
        return Err(SyncError::InvalidChunk(format!("invalid {name}")));
    }
    Ok(())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_and_resumes_out_of_order() {
        let payload = b"abcdefghij";
        let chunks = split_sync_payload("transfer-1", payload, 4).unwrap();
        let mut assembler = SyncChunkAssembler::new(
            "transfer-1",
            payload.len() as u64,
            chunks[0].payload_sha256.clone(),
        )
        .unwrap();
        assert_eq!(
            assembler.insert(chunks[2].clone()).unwrap(),
            SyncChunkInsert::Accepted
        );
        assert_eq!(
            assembler.progress().missing[0],
            SyncChunkRange { offset: 0, len: 8 }
        );
        assembler.insert(chunks[0].clone()).unwrap();
        assembler.insert(chunks[1].clone()).unwrap();
        assert_eq!(assembler.assemble().unwrap(), payload);
    }

    #[test]
    fn duplicate_is_idempotent_and_overlap_is_rejected() {
        let chunks = split_sync_payload("transfer-2", b"abcdefgh", 4).unwrap();
        let mut assembler =
            SyncChunkAssembler::new("transfer-2", 8, chunks[0].payload_sha256.clone()).unwrap();
        assert_eq!(
            assembler.insert(chunks[0].clone()).unwrap(),
            SyncChunkInsert::Accepted
        );
        assert_eq!(
            assembler.insert(chunks[0].clone()).unwrap(),
            SyncChunkInsert::Duplicate
        );
        let mut overlap = chunks[1].clone();
        overlap.offset = 3;
        overlap.chunk_sha256 = sha256_hex(&overlap.payload);
        assert!(matches!(
            assembler.insert(overlap),
            Err(SyncError::ChunkConflict(3))
        ));
    }
}
