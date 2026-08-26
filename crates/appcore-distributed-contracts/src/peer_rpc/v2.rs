// =============================================================================
//        #######
//     ###       ###     F: v2.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Explicit bounded chunk frames for peer RPC protocol version 2.

use appcore_types::{CapabilityName, ClusterId, CoreId, ProtocolVersion, TenantId, TraceContext};
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

mod base64_bytes;

/// Version number of the chunked peer RPC wire contract.
pub const PEER_RPC_PROTOCOL_VERSION_V2: u16 = 2;
/// Authenticated V2 peer query endpoint.
pub const PEER_QUERY_PATH_V2: &str = "/v2/peer/query";
/// Authenticated V2 peer command endpoint.
pub const PEER_COMMAND_PATH_V2: &str = "/v2/peer/command";

/// Direction of one independently integrity-checked stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRpcStreamDirectionV2 {
    /// Payload sent from caller to target.
    Request,
    /// Payload returned from target to caller.
    Response,
}

/// Encoding applied independently to one chunk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRpcChunkEncodingV2 {
    /// Chunk bytes are already decoded.
    Identity,
    /// Chunk bytes use gzip and must be decoded under the declared bound.
    Gzip,
}

/// Reason a partial stream was explicitly cancelled.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRpcStreamCancelReasonV2 {
    /// The caller no longer needs the result.
    Caller,
    /// The stream deadline elapsed.
    Deadline,
    /// Runtime shutdown cancelled accepted work.
    Shutdown,
    /// The underlying transport failed.
    Transport,
}

/// Opens one bounded request or response stream.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcStreamOpenV2 {
    /// Exact distributed protocol version.
    pub protocol_version: ProtocolVersion,
    /// Stable RPC request identity.
    pub request_id: String,
    /// Stable identity of this request or response stream.
    pub stream_id: String,
    /// Trace identity propagated across cores.
    pub trace_id: String,
    /// Whether this stream carries request or response bytes.
    pub direction: PeerRpcStreamDirectionV2,
    /// Query or command semantics inherited from the RPC.
    pub call_kind: super::v1::PeerRpcCallKind,
    /// Core sending this stream.
    pub source_core_id: CoreId,
    /// Core receiving this stream.
    pub target_core_id: CoreId,
    /// Tenant isolation boundary.
    pub tenant_id: TenantId,
    /// Cluster isolation boundary.
    pub cluster_id: ClusterId,
    /// Creation timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Absolute stream deadline in milliseconds.
    pub deadline_ms: u64,
    /// Single-use replay-protection value for the open frame.
    pub nonce: String,
    /// Generic capability being invoked.
    pub capability: CapabilityName,
    /// Exact decoded payload size expected at commit.
    pub payload_bytes: u64,
    /// Maximum decoded bytes in each non-final chunk.
    pub chunk_bytes: u32,
    /// Exact number of chunk frames expected before commit.
    pub chunk_count: u32,
    /// Optional idempotency key for a mutating request.
    pub idempotency_key: Option<String>,
    /// Optional structured trace context.
    pub trace: Option<TraceContext>,
}

impl Debug for PeerRpcStreamOpenV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcStreamOpenV2")
            .field("protocol_version", &self.protocol_version)
            .field("request_id", &self.request_id)
            .field("stream_id", &self.stream_id)
            .field("trace_id", &self.trace_id)
            .field("direction", &self.direction)
            .field("call_kind", &self.call_kind)
            .field("source_core_id", &self.source_core_id)
            .field("target_core_id", &self.target_core_id)
            .field("tenant_id", &self.tenant_id)
            .field("cluster_id", &self.cluster_id)
            .field("timestamp_ms", &self.timestamp_ms)
            .field("deadline_ms", &self.deadline_ms)
            .field("capability", &self.capability)
            .field("payload_bytes", &self.payload_bytes)
            .field("chunk_bytes", &self.chunk_bytes)
            .field("chunk_count", &self.chunk_count)
            .field("has_idempotency_key", &self.idempotency_key.is_some())
            .field("trace", &self.trace)
            .finish()
    }
}

/// Carries one independently bounded and integrity-checked payload chunk.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcStreamChunkV2 {
    /// Exact distributed protocol version.
    pub protocol_version: ProtocolVersion,
    /// RPC request identity from the open frame.
    pub request_id: String,
    /// Stream identity from the open frame.
    pub stream_id: String,
    /// Zero-based sequence number.
    pub sequence: u32,
    /// Per-chunk encoding.
    pub encoding: PeerRpcChunkEncodingV2,
    /// Encoded bytes; debug output never includes their contents.
    #[serde(with = "base64_bytes")]
    pub payload: Vec<u8>,
    /// Exact decoded byte length.
    pub decoded_bytes: u32,
    /// SHA-256 digest of the decoded chunk.
    pub chunk_hash: String,
}

impl Debug for PeerRpcStreamChunkV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcStreamChunkV2")
            .field("protocol_version", &self.protocol_version)
            .field("request_id", &self.request_id)
            .field("stream_id", &self.stream_id)
            .field("sequence", &self.sequence)
            .field("encoding", &self.encoding)
            .field("encoded_bytes", &self.payload.len())
            .field("decoded_bytes", &self.decoded_bytes)
            .field("chunk_hash", &self.chunk_hash)
            .finish()
    }
}

/// Commits a complete stream and binds the digest of all decoded chunks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcStreamCommitV2 {
    /// Exact distributed protocol version.
    pub protocol_version: ProtocolVersion,
    /// RPC request identity from the open frame.
    pub request_id: String,
    /// Stream identity from the open frame.
    pub stream_id: String,
    /// Number of chunks sent before this frame.
    pub chunk_count: u32,
    /// Total decoded payload bytes.
    pub payload_bytes: u64,
    /// SHA-256 digest of the complete decoded payload.
    pub payload_hash: String,
}

/// Cancels one partial stream without committing its payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcStreamCancelV2 {
    /// Exact distributed protocol version.
    pub protocol_version: ProtocolVersion,
    /// RPC request identity from the open frame.
    pub request_id: String,
    /// Stream identity from the open frame.
    pub stream_id: String,
    /// Controlled cancellation reason.
    pub reason: PeerRpcStreamCancelReasonV2,
}

/// Requests the next bounded frame of a response stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcStreamPullV2 {
    /// Exact distributed protocol version.
    pub protocol_version: ProtocolVersion,
    /// RPC request identity from the response open frame.
    pub request_id: String,
    /// Response stream identity.
    pub stream_id: String,
}

/// One explicitly tagged frame in a V2 peer stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "frame_type", content = "frame", rename_all = "snake_case")]
pub enum PeerRpcStreamFrameV2 {
    /// Opens a new bounded stream.
    Open(Box<PeerRpcStreamOpenV2>),
    /// Supplies the next payload chunk.
    Chunk(PeerRpcStreamChunkV2),
    /// Commits all previously supplied chunks.
    Commit(PeerRpcStreamCommitV2),
    /// Cancels a partial stream.
    Cancel(PeerRpcStreamCancelV2),
    /// Pulls the next response frame.
    Pull(PeerRpcStreamPullV2),
}

/// Bounded acknowledgement returned for one V2 frame exchange.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcStreamReplyV2 {
    /// RPC request identity.
    pub request_id: String,
    /// Current request or response stream identity.
    pub stream_id: String,
    /// Next request chunk sequence accepted by the host.
    pub next_sequence: u32,
    /// Aggregate decoded request bytes accepted by the host.
    pub received_bytes: u64,
    /// Optional next response frame after commit or pull.
    pub response_frame: Option<Box<PeerRpcStreamFrameV2>>,
    /// Whether this exchange completed and removed the selected stream.
    pub complete: bool,
}

/// Controlled HTTP error code returned by an explicit V2 frame endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRpcStreamHttpErrorCodeV2 {
    /// The bearer credential is absent or invalid.
    Unauthorized,
    /// The bearer credential is not bound to the supplied frame.
    Forbidden,
    /// The JSON body is malformed or its stream configuration is incoherent.
    InvalidFrame,
    /// The frame does not declare protocol version 2.
    ProtocolMismatch,
    /// Aggregate payload quota was exceeded.
    PayloadTooLarge,
    /// Per-chunk quota was exceeded.
    ChunkTooLarge,
    /// Chunk order is missing, repeated, or reordered.
    InvalidSequence,
    /// Chunk length differs from its declaration.
    InvalidChunkLength,
    /// Chunk integrity validation failed.
    InvalidChunkHash,
    /// Aggregate integrity validation failed.
    InvalidPayloadHash,
    /// Request or stream identity does not match the admitted session.
    IdentityMismatch,
    /// Tenant isolation does not match the target host.
    TenantMismatch,
    /// Cluster isolation does not match the target host.
    ClusterMismatch,
    /// Target Core identity does not match the target host.
    TargetMismatch,
    /// The open-frame nonce was already accepted inside its replay window.
    NonceReplay,
    /// Request and response stream direction was reversed.
    DirectionMismatch,
    /// Query and command endpoint selection does not match the session.
    CallKindMismatch,
    /// Commit arrived before the complete declared payload.
    Incomplete,
    /// Deadline elapsed before the frame completed.
    Expired,
    /// Cooperative cancellation was observed.
    Cancelled,
    /// A bounded stream source or sink failed.
    Io,
    /// Chunk encoding is unsupported or corrupt.
    InvalidEncoding,
    /// A previous failure closed the selected stream.
    Closed,
    /// Partial-state admission capacity is exhausted.
    CapacityExceeded,
    /// The selected V2 routes were not configured by the host.
    EndpointUnavailable,
}

/// Redacted typed error returned by a V2 HTTP frame exchange.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcStreamHttpErrorV2 {
    /// Request identity when a valid frame supplied it.
    pub request_id: Option<String>,
    /// Stream identity when a valid frame supplied it.
    pub stream_id: Option<String>,
    /// Stable controlled failure code.
    pub code: PeerRpcStreamHttpErrorCodeV2,
}

impl PeerRpcStreamFrameV2 {
    /// Returns the request identity carried by this frame.
    pub fn request_id(&self) -> &str {
        match self {
            Self::Open(frame) => &frame.request_id,
            Self::Chunk(frame) => &frame.request_id,
            Self::Commit(frame) => &frame.request_id,
            Self::Cancel(frame) => &frame.request_id,
            Self::Pull(frame) => &frame.request_id,
        }
    }

    /// Returns the request or response stream identity carried by this frame.
    pub fn stream_id(&self) -> &str {
        match self {
            Self::Open(frame) => &frame.stream_id,
            Self::Chunk(frame) => &frame.stream_id,
            Self::Commit(frame) => &frame.stream_id,
            Self::Cancel(frame) => &frame.stream_id,
            Self::Pull(frame) => &frame.stream_id,
        }
    }
}

/// Controlled failure while encoding or receiving a V2 stream.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PeerRpcStreamErrorV2 {
    /// Stream limits or open metadata are incoherent.
    #[error("invalid peer RPC V2 stream configuration")]
    InvalidConfig,
    /// A frame does not declare protocol version 2.
    #[error("peer RPC V2 protocol mismatch")]
    ProtocolMismatch,
    /// Declared or observed aggregate payload exceeds the configured quota.
    #[error("peer RPC V2 payload is too large")]
    PayloadTooLarge,
    /// A chunk exceeds its encoded or decoded bound.
    #[error("peer RPC V2 chunk is too large")]
    ChunkTooLarge,
    /// A chunk is missing, repeated, or out of order.
    #[error("peer RPC V2 chunk sequence is invalid")]
    InvalidSequence,
    /// A chunk length differs from its exact declared length.
    #[error("peer RPC V2 chunk length is invalid")]
    InvalidChunkLength,
    /// A decoded chunk does not match its digest.
    #[error("peer RPC V2 chunk hash is invalid")]
    InvalidChunkHash,
    /// The committed aggregate does not match its digest.
    #[error("peer RPC V2 payload hash is invalid")]
    InvalidPayloadHash,
    /// Stream identity differs from the open frame.
    #[error("peer RPC V2 stream identity mismatch")]
    IdentityMismatch,
    /// A request or response frame was admitted at the opposite stream boundary.
    #[error("peer RPC V2 stream direction mismatch")]
    DirectionMismatch,
    /// A frame used the query endpoint for a command session or the reverse.
    #[error("peer RPC V2 call kind mismatch")]
    CallKindMismatch,
    /// Commit arrived before every declared chunk.
    #[error("peer RPC V2 stream is incomplete")]
    Incomplete,
    /// The stream deadline elapsed.
    #[error("peer RPC V2 stream expired")]
    Expired,
    /// Cooperative cancellation was requested.
    #[error("peer RPC V2 stream cancelled")]
    Cancelled,
    /// A bounded source or sink operation failed.
    #[error("peer RPC V2 stream I/O failed")]
    Io,
    /// Chunk decompression failed or used an unsupported representation.
    #[error("peer RPC V2 chunk encoding is invalid")]
    InvalidEncoding,
    /// A previous invalid frame permanently closed this stream instance.
    #[error("peer RPC V2 stream is closed")]
    Closed,
    /// The bounded partial-stream registry has no admission capacity.
    #[error("peer RPC V2 stream capacity is exhausted")]
    CapacityExceeded,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_frame_has_stable_v2_json_shape() {
        let frame = PeerRpcStreamFrameV2::Open(Box::new(PeerRpcStreamOpenV2 {
            protocol_version: ProtocolVersion::new(2),
            request_id: "req-1".to_string(),
            stream_id: "stream-1".to_string(),
            trace_id: "trace-1".to_string(),
            direction: PeerRpcStreamDirectionV2::Request,
            call_kind: super::super::v1::PeerRpcCallKind::Query,
            source_core_id: CoreId::new("core-a").unwrap(),
            target_core_id: CoreId::new("core-b").unwrap(),
            tenant_id: TenantId::new("tenant-a").unwrap(),
            cluster_id: ClusterId::new("cluster-a").unwrap(),
            timestamp_ms: 10,
            deadline_ms: 100,
            nonce: "nonce-1".to_string(),
            capability: CapabilityName::new("runtime.query").unwrap(),
            payload_bytes: 5,
            chunk_bytes: 3,
            chunk_count: 2,
            idempotency_key: None,
            trace: None,
        }));
        let encoded = serde_json::to_value(frame).unwrap();
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/peer-rpc-stream-open-v2.json")).unwrap();
        assert_eq!(encoded, fixture);
    }

    #[test]
    fn debug_omits_nonce_idempotency_and_chunk_payload() {
        let open = PeerRpcStreamOpenV2 {
            protocol_version: ProtocolVersion::new(2),
            request_id: "req-1".to_string(),
            stream_id: "stream-1".to_string(),
            trace_id: "trace-1".to_string(),
            direction: PeerRpcStreamDirectionV2::Request,
            call_kind: super::super::v1::PeerRpcCallKind::Command,
            source_core_id: CoreId::new("core-a").unwrap(),
            target_core_id: CoreId::new("core-b").unwrap(),
            tenant_id: TenantId::new("tenant-a").unwrap(),
            cluster_id: ClusterId::new("cluster-a").unwrap(),
            timestamp_ms: 10,
            deadline_ms: 100,
            nonce: "private-marker".to_string(),
            capability: CapabilityName::new("runtime.command").unwrap(),
            payload_bytes: 1,
            chunk_bytes: 1,
            chunk_count: 1,
            idempotency_key: Some("private-marker".to_string()),
            trace: None,
        };
        let chunk = PeerRpcStreamChunkV2 {
            protocol_version: ProtocolVersion::new(2),
            request_id: "req-1".to_string(),
            stream_id: "stream-1".to_string(),
            sequence: 0,
            encoding: PeerRpcChunkEncodingV2::Identity,
            payload: b"private-marker".to_vec(),
            decoded_bytes: 14,
            chunk_hash: "hash".to_string(),
        };
        assert!(!format!("{open:?}").contains("private-marker"));
        assert!(!format!("{chunk:?}").contains("private-marker"));
    }

    #[test]
    fn chunk_payload_uses_bounded_base64_json_string() {
        let chunk = PeerRpcStreamFrameV2::Chunk(PeerRpcStreamChunkV2 {
            protocol_version: ProtocolVersion::new(2),
            request_id: "req-1".to_string(),
            stream_id: "stream-1".to_string(),
            sequence: 0,
            encoding: PeerRpcChunkEncodingV2::Identity,
            payload: vec![0, 127, 128, 255],
            decoded_bytes: 4,
            chunk_hash: "hash".to_string(),
        });
        let encoded = serde_json::to_value(&chunk).unwrap();
        assert_eq!(encoded["frame"]["payload"], "AH+A/w==");
        assert_eq!(
            serde_json::from_value::<PeerRpcStreamFrameV2>(encoded).unwrap(),
            chunk
        );
    }
}
