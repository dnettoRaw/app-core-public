// =============================================================================
//        #######
//     ###       ###     F: v1.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 13:21:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 12:48:56 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Peer RPC protocol version 1.

use appcore_types::{CapabilityName, ClusterId, CoreId, ProtocolVersion, TenantId, TraceContext};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::{Debug, Formatter};

mod error;

pub use error::{PeerRpcRemoteErrorCodeV1, PeerRpcRemoteErrorV1};

/// Version number of this peer RPC wire contract.
pub const PEER_RPC_PROTOCOL_VERSION: u16 = 1;
/// Public authenticated peer health endpoint.
pub const PEER_HEALTH_PATH: &str = "/v1/peer/health";
/// Public authenticated peer manifest endpoint.
pub const PEER_MANIFEST_PATH: &str = "/v1/peer/manifest";
/// Authenticated peer query endpoint.
pub const PEER_QUERY_PATH: &str = "/v1/peer/query";
/// Authenticated peer command endpoint.
pub const PEER_COMMAND_PATH: &str = "/v1/peer/command";

/// Authenticated peer request envelope.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcEnvelope {
    /// Stable request identity.
    pub request_id: String,
    /// Trace identity propagated across cores.
    pub trace_id: String,
    /// Distributed protocol version.
    #[serde(default)]
    pub protocol_version: ProtocolVersion,
    /// Core issuing the request.
    pub source_core_id: CoreId,
    /// Core expected to execute the request.
    pub target_core_id: CoreId,
    /// Tenant isolation boundary.
    pub tenant_id: TenantId,
    /// Cluster isolation boundary.
    pub cluster_id: ClusterId,
    /// Creation timestamp in milliseconds.
    pub timestamp_ms: u64,
    /// Expiration timestamp in milliseconds.
    pub expires_at_ms: u64,
    /// Single-use replay-protection value.
    pub nonce: String,
    /// Generic capability being invoked.
    pub capability: CapabilityName,
    /// Opaque application-owned payload.
    pub payload: Vec<u8>,
    /// Optional idempotency key for a mutating request.
    pub idempotency_key: Option<String>,
    /// SHA-256 digest of `payload`.
    pub body_hash: String,
    /// Optional structured trace context.
    pub trace: Option<TraceContext>,
}

impl Debug for PeerRpcEnvelope {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcEnvelope")
            .field("request_id", &self.request_id)
            .field("trace_id", &self.trace_id)
            .field("protocol_version", &self.protocol_version)
            .field("source_core_id", &self.source_core_id)
            .field("target_core_id", &self.target_core_id)
            .field("tenant_id", &self.tenant_id)
            .field("cluster_id", &self.cluster_id)
            .field("timestamp_ms", &self.timestamp_ms)
            .field("expires_at_ms", &self.expires_at_ms)
            .field("capability", &self.capability)
            .field("payload_bytes", &self.payload.len())
            .field("body_hash", &self.body_hash)
            .field("has_idempotency_key", &self.idempotency_key.is_some())
            .field("trace", &self.trace)
            .finish()
    }
}

impl PeerRpcEnvelope {
    /// Creates a versioned envelope and binds its payload digest.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        request_id: impl Into<String>,
        trace_id: impl Into<String>,
        source_core_id: CoreId,
        target_core_id: CoreId,
        tenant_id: TenantId,
        cluster_id: ClusterId,
        timestamp_ms: u64,
        expires_at_ms: u64,
        nonce: impl Into<String>,
        capability: CapabilityName,
        payload: Vec<u8>,
        idempotency_key: Option<String>,
        trace: Option<TraceContext>,
    ) -> Self {
        let body_hash = payload_hash(&payload);
        Self {
            request_id: request_id.into(),
            trace_id: trace_id.into(),
            protocol_version: ProtocolVersion::default(),
            source_core_id,
            target_core_id,
            tenant_id,
            cluster_id,
            timestamp_ms,
            expires_at_ms,
            nonce: nonce.into(),
            capability,
            payload,
            idempotency_key,
            body_hash,
            trace,
        }
    }
}

/// Response returned for a peer query or command.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcResponse {
    /// Whether execution succeeded.
    pub ok: bool,
    /// Request identity echoed by the peer.
    pub request_id: String,
    /// Opaque application-owned response payload.
    pub payload: Vec<u8>,
    /// Controlled failure detail.
    pub error: Option<String>,
}

impl Debug for PeerRpcResponse {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcResponse")
            .field("ok", &self.ok)
            .field("request_id", &self.request_id)
            .field("payload_bytes", &self.payload.len())
            .field("has_error", &self.error.is_some())
            .finish()
    }
}

impl PeerRpcResponse {
    /// Creates a successful response.
    pub fn ok(request_id: impl Into<String>, payload: Vec<u8>) -> Self {
        Self {
            ok: true,
            request_id: request_id.into(),
            payload,
            error: None,
        }
    }

    /// Creates a controlled rejected response.
    pub fn rejected(request_id: impl Into<String>, error: impl Into<String>) -> Self {
        Self {
            ok: false,
            request_id: request_id.into(),
            payload: Vec::new(),
            error: Some(error.into()),
        }
    }
}

/// Provider-independent peer RPC failure.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PeerRpcError {
    /// The request or response exceeds its configured bound.
    #[error("peer RPC payload is too large")]
    PayloadTooLarge,
    /// Authentication credentials are missing or invalid.
    #[error("peer RPC request is unauthorized")]
    Unauthorized,
    /// Credentials are valid but do not authorize this request.
    #[error("peer RPC request is forbidden")]
    Forbidden,
    /// No eligible peer endpoint is available.
    #[error("peer RPC endpoint is unavailable")]
    EndpointUnavailable,
    /// Source and target tenants differ.
    #[error("peer RPC tenant mismatch")]
    TenantMismatch,
    /// Source and target clusters differ.
    #[error("peer RPC cluster mismatch")]
    ClusterMismatch,
    /// The request targets another core.
    #[error("peer RPC target mismatch")]
    TargetMismatch,
    /// Source and target protocol versions are incompatible.
    #[error("peer RPC protocol mismatch")]
    ProtocolMismatch,
    /// The envelope has expired.
    #[error("peer RPC envelope expired")]
    Expired,
    /// The envelope nonce was already accepted.
    #[error("peer RPC nonce replay")]
    NonceReplay,
    /// Replay protection reached its configured bound.
    #[error("peer RPC nonce cache is full")]
    NonceCacheFull,
    /// The payload does not match the envelope digest.
    #[error("peer RPC body hash is invalid")]
    InvalidBodyHash,
    /// The remote endpoint returned an invalid response.
    #[error("invalid peer RPC response: {0}")]
    InvalidResponse(String),
    /// Transport execution failed.
    #[error("peer RPC transport failed: {0}")]
    Transport(String),
    /// The incoming envelope is malformed.
    #[error("invalid peer RPC envelope: {0}")]
    InvalidEnvelope(String),
    /// A V1 peer returned one exact controlled rejection.
    #[error("{0}")]
    RemoteRejected(PeerRpcRemoteErrorV1),
}

/// Kind of direct peer call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRpcCallKind {
    /// Side-effect-free query.
    Query,
    /// Mutating or important command.
    Command,
}

/// Provider-neutral request passed to a peer client executor.
#[derive(Clone, PartialEq, Eq)]
pub struct PeerRpcOutboundRequest {
    /// Stable request identity.
    pub request_id: String,
    /// Core expected to execute the request.
    pub target_core_id: CoreId,
    /// Generic capability being invoked.
    pub capability: CapabilityName,
    /// Opaque application-owned payload.
    pub payload: Vec<u8>,
    /// Optional idempotency key.
    pub idempotency_key: Option<String>,
    /// Optional trace context.
    pub trace: Option<TraceContext>,
}

impl Debug for PeerRpcOutboundRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcOutboundRequest")
            .field("request_id", &self.request_id)
            .field("target_core_id", &self.target_core_id)
            .field("capability", &self.capability)
            .field("payload_bytes", &self.payload.len())
            .field("has_idempotency_key", &self.idempotency_key.is_some())
            .field("trace", &self.trace)
            .finish()
    }
}

impl PeerRpcOutboundRequest {
    /// Creates an outbound peer request.
    pub fn new(
        request_id: impl Into<String>,
        target_core_id: CoreId,
        capability: CapabilityName,
        payload: Vec<u8>,
        idempotency_key: Option<String>,
        trace: Option<TraceContext>,
    ) -> Self {
        Self {
            request_id: request_id.into(),
            target_core_id,
            capability,
            payload,
            idempotency_key,
            trace,
        }
    }
}

/// Provider contract used to invoke a direct peer endpoint.
pub trait PeerRpcClientExecutor: Send + Sync {
    /// Executes one query or command against `endpoint_url`.
    fn call_peer(
        &self,
        endpoint_url: &str,
        kind: PeerRpcCallKind,
        request: PeerRpcOutboundRequest,
    ) -> Result<PeerRpcResponse, PeerRpcError>;
}

/// Response returned by the peer health endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerHealthResponse {
    /// Whether the peer is ready to receive calls.
    pub ok: bool,
    /// Peer core identity.
    pub core_id: CoreId,
    /// Peer tenant boundary.
    pub tenant_id: TenantId,
    /// Peer cluster boundary.
    pub cluster_id: ClusterId,
}

/// Response returned by the peer manifest endpoint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerManifestResponse {
    /// Versioned provider-independent peer advertisement.
    pub advertisement: PeerAdvertisementV1,
}

/// Stable peer advertisement independent of internal core-manifest layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerAdvertisementV1 {
    /// Advertisement schema version.
    pub schema_version: u16,
    /// Distributed and Runtime identity fields required for compatibility.
    pub identity: PeerIdentityV1,
    /// Human-readable application name.
    pub app_name: String,
    /// Application version.
    pub app_version: String,
    /// Minimum compatible Runtime version.
    pub runtime_min_version: String,
    /// Optional maximum compatible Runtime version.
    pub runtime_max_version: Option<String>,
    /// Generic capabilities exposed by this peer.
    pub capabilities: Vec<PeerCapabilityV1>,
    /// Public network endpoints without credentials.
    pub endpoints: Vec<PeerEndpointV1>,
    /// Non-sensitive routing metadata.
    pub metadata: BTreeMap<String, String>,
}

/// Identity fields carried by a V1 peer advertisement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerIdentityV1 {
    /// Tenant isolation boundary.
    pub tenant_id: String,
    /// Cluster isolation boundary.
    pub cluster_id: String,
    /// Stable logical core identity.
    pub core_id: String,
    /// Unique running instance identity.
    pub instance_id: String,
    /// Generic core role.
    pub kind: String,
    /// Distributed protocol version.
    pub protocol_version: u16,
    /// Application identity.
    pub app_id: String,
    /// Compatible application family.
    pub app_family: String,
    /// Sync compatibility group.
    pub sync_group: String,
    /// Runtime contract version.
    pub runtime_contract: u16,
    /// Runtime node identity.
    pub node_id: String,
}

/// Generic capability advertised by a peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerCapabilityV1 {
    /// Stable capability name.
    pub name: String,
    /// Capability contract version.
    pub version: String,
    /// `query`, `command`, or `stream`.
    pub mode: String,
    /// `local`, `cluster`, or `tenant`.
    pub visibility: String,
    /// Whether service leadership is required.
    pub requires_leader: bool,
    /// Whether the capability is read-only.
    pub read_only: bool,
    /// Whether mutating requests require idempotency.
    pub idempotency_required: bool,
}

/// Public endpoint advertised by a peer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerEndpointV1 {
    /// Logical endpoint name.
    pub name: String,
    /// Public endpoint URL.
    pub url: String,
    /// Transport protocol identifier.
    pub protocol: String,
    /// Non-sensitive endpoint metadata.
    pub metadata: BTreeMap<String, String>,
}

fn payload_hash(payload: &[u8]) -> String {
    let digest = Sha256::digest(payload);
    let mut output = String::with_capacity(digest.len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for byte in digest {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_has_stable_v1_json_shape() {
        let envelope = PeerRpcEnvelope::new(
            "req-1",
            "trace-1",
            CoreId::new("core-a").unwrap(),
            CoreId::new("core-b").unwrap(),
            TenantId::new("tenant-a").unwrap(),
            ClusterId::new("cluster-a").unwrap(),
            10,
            20,
            "nonce-1",
            CapabilityName::new("runtime.query").unwrap(),
            b"hello".to_vec(),
            None,
            None,
        );
        let encoded = serde_json::to_value(envelope).unwrap();
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../fixtures/peer-rpc-envelope-v1.json")).unwrap();
        assert_eq!(encoded, fixture);
    }

    #[test]
    fn peer_debug_omits_opaque_payloads_and_error_details() {
        let marker = b"secret-marker-must-not-appear";
        let envelope = PeerRpcEnvelope::new(
            "req-1",
            "trace-1",
            CoreId::new("core-a").unwrap(),
            CoreId::new("core-b").unwrap(),
            TenantId::new("tenant-a").unwrap(),
            ClusterId::new("cluster-a").unwrap(),
            10,
            20,
            "nonce-secret-marker-must-not-appear",
            CapabilityName::new("runtime.query").unwrap(),
            marker.to_vec(),
            Some("secret-marker-must-not-appear".to_string()),
            None,
        );
        let response = PeerRpcResponse::rejected("req-1", "secret-marker-must-not-appear");
        let outbound = PeerRpcOutboundRequest::new(
            "req-1",
            CoreId::new("core-b").unwrap(),
            CapabilityName::new("runtime.query").unwrap(),
            marker.to_vec(),
            Some("secret-marker-must-not-appear".to_string()),
            None,
        );

        assert!(!format!("{envelope:?}").contains("secret-marker-must-not-appear"));
        assert!(!format!("{response:?}").contains("secret-marker-must-not-appear"));
        assert!(!format!("{outbound:?}").contains("secret-marker-must-not-appear"));
    }
}
