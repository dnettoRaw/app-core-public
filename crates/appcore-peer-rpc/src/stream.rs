// =============================================================================
//        #######
//     ###       ###     F: stream.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Incremental V2 peer stream encoder and receiver.

use super::*;
use crate::v2::{
    PeerRpcChunkEncodingV2, PeerRpcStreamChunkV2, PeerRpcStreamCommitV2, PeerRpcStreamErrorV2,
    PeerRpcStreamFrameV2, PeerRpcStreamOpenV2, PEER_RPC_PROTOCOL_VERSION_V2,
};
use appcore_transport::{decode_gzip_limited, encode_gzip_if_smaller, TransportError};
use std::io::{Read, Write};

/// Explicit aggregate and per-chunk limits for a V2 peer stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerRpcChunkLimits {
    /// Maximum decoded bytes in one chunk.
    pub max_chunk_bytes: usize,
    /// Maximum encoded bytes accepted before decompression.
    pub max_encoded_chunk_bytes: usize,
    /// Maximum decoded bytes across the complete stream.
    pub max_payload_bytes: u64,
    /// Maximum chunk frames across the complete stream.
    pub max_chunks: u32,
}

impl Default for PeerRpcChunkLimits {
    fn default() -> Self {
        Self {
            max_chunk_bytes: 64 * 1024,
            max_encoded_chunk_bytes: 96 * 1024,
            max_payload_bytes: 64 * 1024 * 1024,
            max_chunks: 1_024,
        }
    }
}

impl PeerRpcChunkLimits {
    fn validate(&self) -> Result<(), PeerRpcStreamErrorV2> {
        if self.max_chunk_bytes == 0
            || self.max_encoded_chunk_bytes < self.max_chunk_bytes
            || self.max_payload_bytes == 0
            || self.max_chunks == 0
            || self.max_chunk_bytes > u32::MAX as usize
        {
            return Err(PeerRpcStreamErrorV2::InvalidConfig);
        }
        Ok(())
    }

    fn validate_open(
        &self,
        open: &PeerRpcStreamOpenV2,
        now_ms: u64,
    ) -> Result<(), PeerRpcStreamErrorV2> {
        self.validate()?;
        validate_open_identity(open)?;
        if open.protocol_version.as_u16() != PEER_RPC_PROTOCOL_VERSION_V2 {
            return Err(PeerRpcStreamErrorV2::ProtocolMismatch);
        }
        if open.timestamp_ms >= open.deadline_ms || now_ms >= open.deadline_ms {
            return Err(PeerRpcStreamErrorV2::Expired);
        }
        if open.payload_bytes > self.max_payload_bytes {
            return Err(PeerRpcStreamErrorV2::PayloadTooLarge);
        }
        let chunk_bytes = open.chunk_bytes as usize;
        if chunk_bytes == 0 || chunk_bytes > self.max_chunk_bytes {
            return Err(PeerRpcStreamErrorV2::ChunkTooLarge);
        }
        let expected_chunks = expected_chunk_count(open.payload_bytes, open.chunk_bytes)?;
        if open.chunk_count != expected_chunks || open.chunk_count > self.max_chunks {
            return Err(PeerRpcStreamErrorV2::InvalidConfig);
        }
        Ok(())
    }
}

/// Incrementally reads a bounded source and emits explicit V2 frames.
pub struct PeerRpcChunkEncoder<R> {
    open: PeerRpcStreamOpenV2,
    source: R,
    limits: PeerRpcChunkLimits,
    cancellation: CancellationToken,
    hasher: Sha256,
    sequence: u32,
    emitted_bytes: u64,
    emitted_open: bool,
    emitted_commit: bool,
    closed: bool,
}

impl<R> PeerRpcChunkEncoder<R>
where
    R: Read,
{
    /// Creates an encoder after validating all declared aggregate bounds.
    pub fn new(
        open: PeerRpcStreamOpenV2,
        source: R,
        limits: PeerRpcChunkLimits,
        cancellation: CancellationToken,
        now_ms: u64,
    ) -> Result<Self, PeerRpcStreamErrorV2> {
        limits.validate_open(&open, now_ms)?;
        Ok(Self {
            open,
            source,
            limits,
            cancellation,
            hasher: Sha256::new(),
            sequence: 0,
            emitted_bytes: 0,
            emitted_open: false,
            emitted_commit: false,
            closed: false,
        })
    }

    /// Emits the next open, chunk, or commit frame without reading unbounded data.
    pub fn next_frame(
        &mut self,
        now_ms: u64,
    ) -> Result<Option<PeerRpcStreamFrameV2>, PeerRpcStreamErrorV2> {
        self.check_active(now_ms)?;
        if !self.emitted_open {
            self.emitted_open = true;
            return Ok(Some(PeerRpcStreamFrameV2::Open(Box::new(
                self.open.clone(),
            ))));
        }
        if self.sequence < self.open.chunk_count {
            let result = self.encode_next_chunk().map(Some);
            if result.is_err() {
                self.closed = true;
            }
            return result;
        }
        if !self.emitted_commit {
            self.reject_trailing_source_bytes()?;
            self.emitted_commit = true;
            return Ok(Some(PeerRpcStreamFrameV2::Commit(PeerRpcStreamCommitV2 {
                protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
                request_id: self.open.request_id.clone(),
                stream_id: self.open.stream_id.clone(),
                chunk_count: self.sequence,
                payload_bytes: self.emitted_bytes,
                payload_hash: hex_encode(&self.hasher.clone().finalize()),
            })));
        }
        Ok(None)
    }

    fn encode_next_chunk(&mut self) -> Result<PeerRpcStreamFrameV2, PeerRpcStreamErrorV2> {
        let expected = expected_chunk_length(&self.open, self.sequence)?;
        let decoded = read_exact_bounded(&mut self.source, expected)?;
        let chunk_hash = payload_hash(&decoded);
        self.hasher.update(&decoded);
        self.emitted_bytes = self.emitted_bytes.saturating_add(decoded.len() as u64);
        let (encoding, payload) = encode_chunk(&decoded, &self.limits)?;
        let chunk = PeerRpcStreamChunkV2 {
            protocol_version: ProtocolVersion::new(PEER_RPC_PROTOCOL_VERSION_V2),
            request_id: self.open.request_id.clone(),
            stream_id: self.open.stream_id.clone(),
            sequence: self.sequence,
            encoding,
            payload,
            decoded_bytes: decoded.len() as u32,
            chunk_hash,
        };
        self.sequence = self.sequence.saturating_add(1);
        Ok(PeerRpcStreamFrameV2::Chunk(chunk))
    }

    fn reject_trailing_source_bytes(&mut self) -> Result<(), PeerRpcStreamErrorV2> {
        let mut trailing = [0u8; 1];
        match self.source.read(&mut trailing) {
            Ok(0) => Ok(()),
            Ok(_) => self.fail(PeerRpcStreamErrorV2::PayloadTooLarge),
            Err(_) => self.fail(PeerRpcStreamErrorV2::Io),
        }
    }

    fn check_active(&self, now_ms: u64) -> Result<(), PeerRpcStreamErrorV2> {
        if self.closed {
            return Err(PeerRpcStreamErrorV2::Closed);
        }
        if self.cancellation.is_cancelled() {
            return Err(PeerRpcStreamErrorV2::Cancelled);
        }
        if now_ms >= self.open.deadline_ms {
            return Err(PeerRpcStreamErrorV2::Expired);
        }
        Ok(())
    }

    fn fail<T>(&mut self, error: PeerRpcStreamErrorV2) -> Result<T, PeerRpcStreamErrorV2> {
        self.closed = true;
        Err(error)
    }
}

/// Incrementally verifies chunks and writes decoded bytes to a bounded sink.
pub struct PeerRpcChunkAssembler<W> {
    open: PeerRpcStreamOpenV2,
    sink: W,
    limits: PeerRpcChunkLimits,
    cancellation: CancellationToken,
    hasher: Sha256,
    next_sequence: u32,
    received_bytes: u64,
    closed: bool,
}

impl<W> PeerRpcChunkAssembler<W>
where
    W: Write,
{
    /// Opens a receiver after validating protocol, identity, deadline, and quotas.
    pub fn new(
        open: PeerRpcStreamOpenV2,
        sink: W,
        limits: PeerRpcChunkLimits,
        cancellation: CancellationToken,
        now_ms: u64,
    ) -> Result<Self, PeerRpcStreamErrorV2> {
        limits.validate_open(&open, now_ms)?;
        Ok(Self {
            open,
            sink,
            limits,
            cancellation,
            hasher: Sha256::new(),
            next_sequence: 0,
            received_bytes: 0,
            closed: false,
        })
    }

    /// Verifies and writes exactly one expected chunk.
    pub fn push_chunk(
        &mut self,
        chunk: PeerRpcStreamChunkV2,
        now_ms: u64,
    ) -> Result<(), PeerRpcStreamErrorV2> {
        let result = self.push_chunk_inner(chunk, now_ms);
        if result.is_err() {
            self.closed = true;
        }
        result
    }

    /// Commits the stream and returns its sink only after aggregate integrity succeeds.
    pub fn finish(
        mut self,
        commit: PeerRpcStreamCommitV2,
        now_ms: u64,
    ) -> Result<W, PeerRpcStreamErrorV2> {
        self.check_active(now_ms)?;
        validate_frame_identity(
            &self.open,
            commit.protocol_version,
            &commit.request_id,
            &commit.stream_id,
        )?;
        if self.next_sequence != self.open.chunk_count
            || commit.chunk_count != self.open.chunk_count
            || self.received_bytes != self.open.payload_bytes
            || commit.payload_bytes != self.open.payload_bytes
        {
            return Err(PeerRpcStreamErrorV2::Incomplete);
        }
        if commit.payload_hash != hex_encode(&self.hasher.finalize()) {
            return Err(PeerRpcStreamErrorV2::InvalidPayloadHash);
        }
        self.sink.flush().map_err(|_| PeerRpcStreamErrorV2::Io)?;
        Ok(self.sink)
    }

    /// Returns the next exact sequence number expected by this receiver.
    pub fn next_sequence(&self) -> u32 {
        self.next_sequence
    }

    /// Returns immutable validated stream metadata.
    pub fn open(&self) -> &PeerRpcStreamOpenV2 {
        &self.open
    }

    /// Returns aggregate decoded bytes accepted so far.
    pub fn received_bytes(&self) -> u64 {
        self.received_bytes
    }

    fn push_chunk_inner(
        &mut self,
        chunk: PeerRpcStreamChunkV2,
        now_ms: u64,
    ) -> Result<(), PeerRpcStreamErrorV2> {
        self.check_active(now_ms)?;
        validate_frame_identity(
            &self.open,
            chunk.protocol_version,
            &chunk.request_id,
            &chunk.stream_id,
        )?;
        if chunk.sequence != self.next_sequence || chunk.sequence >= self.open.chunk_count {
            return Err(PeerRpcStreamErrorV2::InvalidSequence);
        }
        let expected = expected_chunk_length(&self.open, chunk.sequence)?;
        if chunk.decoded_bytes as usize != expected {
            return Err(PeerRpcStreamErrorV2::InvalidChunkLength);
        }
        if chunk.payload.len() > self.limits.max_encoded_chunk_bytes {
            return Err(PeerRpcStreamErrorV2::ChunkTooLarge);
        }
        let decoded = decode_chunk(&chunk, expected)?;
        if decoded.len() != expected {
            return Err(PeerRpcStreamErrorV2::InvalidChunkLength);
        }
        if payload_hash(&decoded) != chunk.chunk_hash {
            return Err(PeerRpcStreamErrorV2::InvalidChunkHash);
        }
        let next_total = self.received_bytes.saturating_add(decoded.len() as u64);
        if next_total > self.open.payload_bytes || next_total > self.limits.max_payload_bytes {
            return Err(PeerRpcStreamErrorV2::PayloadTooLarge);
        }
        self.sink
            .write_all(&decoded)
            .map_err(|_| PeerRpcStreamErrorV2::Io)?;
        self.hasher.update(&decoded);
        self.received_bytes = next_total;
        self.next_sequence = self.next_sequence.saturating_add(1);
        Ok(())
    }

    fn check_active(&self, now_ms: u64) -> Result<(), PeerRpcStreamErrorV2> {
        if self.closed {
            return Err(PeerRpcStreamErrorV2::Closed);
        }
        if self.cancellation.is_cancelled() {
            return Err(PeerRpcStreamErrorV2::Cancelled);
        }
        if now_ms >= self.open.deadline_ms {
            return Err(PeerRpcStreamErrorV2::Expired);
        }
        Ok(())
    }
}

fn validate_open_identity(open: &PeerRpcStreamOpenV2) -> Result<(), PeerRpcStreamErrorV2> {
    for (kind, value) in [
        ("PeerRequestId", open.request_id.as_str()),
        ("PeerStreamId", open.stream_id.as_str()),
        ("TraceId", open.trace_id.as_str()),
        ("PeerNonce", open.nonce.as_str()),
    ] {
        validate_identifier(kind, value).map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
    }
    if let Some(key) = &open.idempotency_key {
        validate_identifier("IdempotencyKey", key)
            .map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)?;
    }
    open.source_core_id
        .validate()
        .and_then(|_| open.target_core_id.validate())
        .and_then(|_| open.tenant_id.validate())
        .and_then(|_| open.cluster_id.validate())
        .and_then(|_| open.capability.validate())
        .map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)
}

fn validate_frame_identity(
    open: &PeerRpcStreamOpenV2,
    protocol: ProtocolVersion,
    request_id: &str,
    stream_id: &str,
) -> Result<(), PeerRpcStreamErrorV2> {
    if protocol.as_u16() != PEER_RPC_PROTOCOL_VERSION_V2 {
        return Err(PeerRpcStreamErrorV2::ProtocolMismatch);
    }
    if request_id != open.request_id || stream_id != open.stream_id {
        return Err(PeerRpcStreamErrorV2::IdentityMismatch);
    }
    Ok(())
}

fn expected_chunk_count(payload_bytes: u64, chunk_bytes: u32) -> Result<u32, PeerRpcStreamErrorV2> {
    if chunk_bytes == 0 {
        return Err(PeerRpcStreamErrorV2::InvalidConfig);
    }
    if payload_bytes == 0 {
        return Ok(0);
    }
    let count = payload_bytes
        .saturating_sub(1)
        .checked_div(u64::from(chunk_bytes))
        .and_then(|value| value.checked_add(1))
        .ok_or(PeerRpcStreamErrorV2::InvalidConfig)?;
    u32::try_from(count).map_err(|_| PeerRpcStreamErrorV2::InvalidConfig)
}

fn expected_chunk_length(
    open: &PeerRpcStreamOpenV2,
    sequence: u32,
) -> Result<usize, PeerRpcStreamErrorV2> {
    if sequence >= open.chunk_count {
        return Err(PeerRpcStreamErrorV2::InvalidSequence);
    }
    let offset = u64::from(sequence).saturating_mul(u64::from(open.chunk_bytes));
    let remaining = open.payload_bytes.saturating_sub(offset);
    usize::try_from(remaining.min(u64::from(open.chunk_bytes)))
        .map_err(|_| PeerRpcStreamErrorV2::ChunkTooLarge)
}

fn read_exact_bounded<R: Read>(
    source: &mut R,
    expected: usize,
) -> Result<Vec<u8>, PeerRpcStreamErrorV2> {
    let mut output = vec![0u8; expected];
    let mut offset = 0;
    while offset < expected {
        match source.read(&mut output[offset..]) {
            Ok(0) => return Err(PeerRpcStreamErrorV2::Incomplete),
            Ok(read) => offset = offset.saturating_add(read),
            Err(_) => return Err(PeerRpcStreamErrorV2::Io),
        }
    }
    Ok(output)
}

fn encode_chunk(
    decoded: &[u8],
    limits: &PeerRpcChunkLimits,
) -> Result<(PeerRpcChunkEncodingV2, Vec<u8>), PeerRpcStreamErrorV2> {
    let compressed = encode_gzip_if_smaller(decoded).map_err(|_| PeerRpcStreamErrorV2::Io)?;
    if let Some(compressed) = compressed {
        if compressed.len() <= limits.max_encoded_chunk_bytes {
            return Ok((PeerRpcChunkEncodingV2::Gzip, compressed));
        }
    }
    Ok((PeerRpcChunkEncodingV2::Identity, decoded.to_vec()))
}

fn decode_chunk(
    chunk: &PeerRpcStreamChunkV2,
    expected: usize,
) -> Result<Vec<u8>, PeerRpcStreamErrorV2> {
    match chunk.encoding {
        PeerRpcChunkEncodingV2::Identity => Ok(chunk.payload.clone()),
        PeerRpcChunkEncodingV2::Gzip => {
            decode_gzip_limited(&chunk.payload, expected).map_err(|error| match error {
                TransportError::ResponseTooLarge { .. } => PeerRpcStreamErrorV2::ChunkTooLarge,
                _ => PeerRpcStreamErrorV2::InvalidEncoding,
            })
        }
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}
