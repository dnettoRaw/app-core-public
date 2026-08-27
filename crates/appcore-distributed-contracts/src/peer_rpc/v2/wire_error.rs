// =============================================================================
//        #######
//     ###       ###     F: wire_error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Bounded, redacted and internally coherent Peer RPC V2 wire rejections.

use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};

/// Maximum encoded retry delay accepted from a peer.
pub const MAX_PEER_RPC_RETRY_AFTER_MS_V2: u64 = 300_000;
/// Maximum correlation identity length accepted from a peer.
pub const MAX_PEER_RPC_CORRELATION_ID_BYTES_V2: usize = 128;
/// Maximum controlled error message length accepted from a peer.
pub const MAX_PEER_RPC_ERROR_MESSAGE_BYTES_V2: usize = 256;

/// Boundary at which a Peer RPC V2 rejection occurred.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRpcWireErrorPhaseV2 {
    /// Credential verification failed.
    Authentication,
    /// Policy rejected an authenticated caller.
    Authorization,
    /// Bounded work admission was unavailable.
    Admission,
    /// Untrusted request metadata or content was invalid.
    Validation,
    /// Accepted work failed during controlled execution.
    Execution,
    /// Deadline, cancellation or closed-state handling terminated work.
    Cancellation,
    /// An unrecognized code was decoded conservatively.
    Unknown,
}

/// Stable controlled error code returned by an explicit Peer RPC V2 endpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PeerRpcWireErrorCodeV2 {
    /// The bearer credential is absent or invalid.
    Unauthorized,
    /// The bearer credential is not bound to the supplied frame.
    Forbidden,
    /// The selected endpoint cannot currently admit work.
    EndpointUnavailable,
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
    /// The peer supplied a code outside this protocol version.
    #[serde(other)]
    Unknown,
}

impl PeerRpcWireErrorCodeV2 {
    /// Returns the only valid phase for this code.
    pub const fn phase(self) -> PeerRpcWireErrorPhaseV2 {
        use PeerRpcWireErrorPhaseV2 as Phase;
        match self {
            Self::Unauthorized => Phase::Authentication,
            Self::Forbidden => Phase::Authorization,
            Self::EndpointUnavailable | Self::CapacityExceeded => Phase::Admission,
            Self::Io => Phase::Execution,
            Self::Expired | Self::Cancelled | Self::Closed => Phase::Cancellation,
            Self::Unknown => Phase::Unknown,
            _ => Phase::Validation,
        }
    }

    /// Reports whether the code permits a bounded higher-level retry.
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::EndpointUnavailable | Self::CapacityExceeded | Self::Io
        )
    }

    const fn retry_after_ms(self) -> Option<u64> {
        match self {
            Self::EndpointUnavailable => Some(250),
            Self::CapacityExceeded => Some(100),
            _ => None,
        }
    }

    const fn message(self) -> &'static str {
        match self {
            Self::Unauthorized => "peer authentication failed",
            Self::Forbidden => "peer authorization failed",
            Self::EndpointUnavailable => "peer endpoint is unavailable",
            Self::InvalidFrame => "peer frame is invalid",
            Self::ProtocolMismatch => "peer protocol is incompatible",
            Self::PayloadTooLarge => "peer payload limit was exceeded",
            Self::ChunkTooLarge => "peer chunk limit was exceeded",
            Self::InvalidSequence => "peer chunk sequence is invalid",
            Self::InvalidChunkLength => "peer chunk length is invalid",
            Self::InvalidChunkHash => "peer chunk integrity failed",
            Self::InvalidPayloadHash => "peer payload integrity failed",
            Self::IdentityMismatch => "peer stream identity is invalid",
            Self::TenantMismatch => "peer tenant isolation failed",
            Self::ClusterMismatch => "peer cluster isolation failed",
            Self::TargetMismatch => "peer target identity failed",
            Self::NonceReplay => "peer replay validation failed",
            Self::DirectionMismatch => "peer stream direction is invalid",
            Self::CallKindMismatch => "peer call kind is invalid",
            Self::Incomplete => "peer stream is incomplete",
            Self::Expired => "peer stream expired",
            Self::Cancelled => "peer stream was cancelled",
            Self::Io => "peer execution failed",
            Self::InvalidEncoding => "peer chunk encoding is invalid",
            Self::Closed => "peer stream is closed",
            Self::CapacityExceeded => "peer admission capacity was exceeded",
            Self::Unknown => "peer rejected the request",
        }
    }
}

/// Redacted typed error returned by an explicit Peer RPC V2 endpoint.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeerRpcWireErrorV2 {
    /// Request identity when it passed bounded identifier validation.
    pub request_id: Option<String>,
    /// Stream identity when it passed bounded identifier validation.
    pub stream_id: Option<String>,
    /// Stable controlled failure code.
    pub code: PeerRpcWireErrorCodeV2,
    /// Whether a safe higher-level operation may retry.
    pub retryable: bool,
    /// Boundary at which the rejection occurred.
    pub phase: PeerRpcWireErrorPhaseV2,
    /// Optional bounded delay before a higher-level retry.
    pub retry_after_ms: Option<u64>,
    /// Optional bounded operational correlation identity.
    pub correlation_id: Option<String>,
    /// Protocol-owned redacted message.
    pub message: String,
}

impl PeerRpcWireErrorV2 {
    /// Creates one internally coherent rejection and drops unsafe identities.
    pub fn controlled(
        request_id: Option<String>,
        stream_id: Option<String>,
        code: PeerRpcWireErrorCodeV2,
    ) -> Self {
        let request_id = request_id.filter(|value| valid_identifier(value));
        let stream_id = stream_id.filter(|value| valid_identifier(value));
        Self {
            correlation_id: request_id.clone(),
            request_id,
            stream_id,
            code,
            retryable: code.retryable(),
            phase: code.phase(),
            retry_after_ms: code.retry_after_ms(),
            message: code.message().to_string(),
        }
    }

    /// Validates known metadata and normalizes unknown codes conservatively.
    pub fn validated(mut self) -> Result<Self, PeerRpcWireErrorValidationErrorV2> {
        validate_optional_identifier(self.request_id.as_deref())?;
        validate_optional_identifier(self.stream_id.as_deref())?;
        validate_optional_identifier(self.correlation_id.as_deref())?;
        if self.message.len() > MAX_PEER_RPC_ERROR_MESSAGE_BYTES_V2 {
            return Err(PeerRpcWireErrorValidationErrorV2::InvalidMessage);
        }
        if self.code == PeerRpcWireErrorCodeV2::Unknown {
            self.retryable = false;
            self.phase = PeerRpcWireErrorPhaseV2::Unknown;
            self.retry_after_ms = None;
            self.message = self.code.message().to_string();
            return Ok(self);
        }
        if self.phase != self.code.phase()
            || self.retryable != self.code.retryable()
            || self.message != self.code.message()
        {
            return Err(PeerRpcWireErrorValidationErrorV2::ContradictoryMetadata);
        }
        if self
            .retry_after_ms
            .is_some_and(|delay| !self.retryable || delay > MAX_PEER_RPC_RETRY_AFTER_MS_V2)
        {
            return Err(PeerRpcWireErrorValidationErrorV2::InvalidRetryAfter);
        }
        Ok(self)
    }
}

impl Debug for PeerRpcWireErrorV2 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcWireErrorV2")
            .field("code", &self.code)
            .field("retryable", &self.retryable)
            .field("phase", &self.phase)
            .field("retry_after_ms", &self.retry_after_ms)
            .field("has_request_id", &self.request_id.is_some())
            .field("has_stream_id", &self.stream_id.is_some())
            .field("has_correlation_id", &self.correlation_id.is_some())
            .field("message_bytes", &self.message.len())
            .finish()
    }
}

/// Validation failure for an untrusted Peer RPC V2 rejection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PeerRpcWireErrorValidationErrorV2 {
    /// An identity is empty, oversized or contains non-identifier bytes.
    #[error("peer RPC V2 error identity is invalid")]
    InvalidIdentity,
    /// Known code, phase, retryability or message metadata disagree.
    #[error("peer RPC V2 error metadata is contradictory")]
    ContradictoryMetadata,
    /// A retry delay is forbidden or exceeds the protocol bound.
    #[error("peer RPC V2 retry delay is invalid")]
    InvalidRetryAfter,
    /// The controlled message exceeds its fixed wire bound.
    #[error("peer RPC V2 error message is invalid")]
    InvalidMessage,
}

fn validate_optional_identifier(
    value: Option<&str>,
) -> Result<(), PeerRpcWireErrorValidationErrorV2> {
    if value.is_some_and(|value| !valid_identifier(value)) {
        return Err(PeerRpcWireErrorValidationErrorV2::InvalidIdentity);
    }
    Ok(())
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_PEER_RPC_CORRELATION_ID_BYTES_V2
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn controlled_error_uses_authoritative_matrix() {
        let error = PeerRpcWireErrorV2::controlled(
            Some("request-1".to_string()),
            Some("stream-1".to_string()),
            PeerRpcWireErrorCodeV2::CapacityExceeded,
        );
        assert!(error.retryable);
        assert_eq!(error.phase, PeerRpcWireErrorPhaseV2::Admission);
        assert_eq!(error.retry_after_ms, Some(100));
        assert_eq!(error.correlation_id.as_deref(), Some("request-1"));
        assert!(error.validated().is_ok());
    }

    #[test]
    fn every_v2_code_produces_valid_authoritative_metadata() {
        let codes = [
            PeerRpcWireErrorCodeV2::Unauthorized,
            PeerRpcWireErrorCodeV2::Forbidden,
            PeerRpcWireErrorCodeV2::EndpointUnavailable,
            PeerRpcWireErrorCodeV2::InvalidFrame,
            PeerRpcWireErrorCodeV2::ProtocolMismatch,
            PeerRpcWireErrorCodeV2::PayloadTooLarge,
            PeerRpcWireErrorCodeV2::ChunkTooLarge,
            PeerRpcWireErrorCodeV2::InvalidSequence,
            PeerRpcWireErrorCodeV2::InvalidChunkLength,
            PeerRpcWireErrorCodeV2::InvalidChunkHash,
            PeerRpcWireErrorCodeV2::InvalidPayloadHash,
            PeerRpcWireErrorCodeV2::IdentityMismatch,
            PeerRpcWireErrorCodeV2::TenantMismatch,
            PeerRpcWireErrorCodeV2::ClusterMismatch,
            PeerRpcWireErrorCodeV2::TargetMismatch,
            PeerRpcWireErrorCodeV2::NonceReplay,
            PeerRpcWireErrorCodeV2::DirectionMismatch,
            PeerRpcWireErrorCodeV2::CallKindMismatch,
            PeerRpcWireErrorCodeV2::Incomplete,
            PeerRpcWireErrorCodeV2::Expired,
            PeerRpcWireErrorCodeV2::Cancelled,
            PeerRpcWireErrorCodeV2::Io,
            PeerRpcWireErrorCodeV2::InvalidEncoding,
            PeerRpcWireErrorCodeV2::Closed,
            PeerRpcWireErrorCodeV2::CapacityExceeded,
            PeerRpcWireErrorCodeV2::Unknown,
        ];
        for code in codes {
            let error =
                PeerRpcWireErrorV2::controlled(Some("request-matrix".to_string()), None, code);
            assert_eq!(error.phase, code.phase());
            assert_eq!(error.retryable, code.retryable());
            assert!(error.validated().is_ok());
        }
    }

    #[test]
    fn contradictory_known_metadata_is_rejected() {
        let mut error = PeerRpcWireErrorV2::controlled(
            Some("request-1".to_string()),
            None,
            PeerRpcWireErrorCodeV2::Forbidden,
        );
        error.retryable = true;
        assert_eq!(
            error.validated(),
            Err(PeerRpcWireErrorValidationErrorV2::ContradictoryMetadata)
        );
    }

    #[test]
    fn unknown_code_is_normalized_without_remote_message_or_retry() {
        let encoded = r#"{"request_id":"request-1","stream_id":null,"code":"future_secret_code","retryable":true,"phase":"admission","retry_after_ms":1,"correlation_id":"request-1","message":"private-marker"}"#;
        let error = serde_json::from_str::<PeerRpcWireErrorV2>(encoded)
            .unwrap()
            .validated()
            .unwrap();
        assert_eq!(error.code, PeerRpcWireErrorCodeV2::Unknown);
        assert!(!error.retryable);
        assert_eq!(error.phase, PeerRpcWireErrorPhaseV2::Unknown);
        assert_eq!(error.retry_after_ms, None);
        assert!(!error.message.contains("private-marker"));
    }

    #[test]
    fn identity_and_retry_bounds_fail_closed() {
        let oversized = "a".repeat(MAX_PEER_RPC_CORRELATION_ID_BYTES_V2 + 1);
        let mut error = PeerRpcWireErrorV2::controlled(
            Some("request-1".to_string()),
            None,
            PeerRpcWireErrorCodeV2::CapacityExceeded,
        );
        error.correlation_id = Some(oversized);
        assert_eq!(
            error.validated(),
            Err(PeerRpcWireErrorValidationErrorV2::InvalidIdentity)
        );

        let mut error = PeerRpcWireErrorV2::controlled(
            Some("request-1".to_string()),
            None,
            PeerRpcWireErrorCodeV2::CapacityExceeded,
        );
        error.retry_after_ms = Some(MAX_PEER_RPC_RETRY_AFTER_MS_V2 + 1);
        assert_eq!(
            error.validated(),
            Err(PeerRpcWireErrorValidationErrorV2::InvalidRetryAfter)
        );

        let mut error = PeerRpcWireErrorV2::controlled(
            Some("request-1".to_string()),
            None,
            PeerRpcWireErrorCodeV2::Unknown,
        );
        error.message = "a".repeat(MAX_PEER_RPC_ERROR_MESSAGE_BYTES_V2 + 1);
        assert_eq!(
            error.validated(),
            Err(PeerRpcWireErrorValidationErrorV2::InvalidMessage)
        );
    }

    #[test]
    fn debug_omits_identity_and_message() {
        let error = PeerRpcWireErrorV2::controlled(
            Some("private-request-marker".to_string()),
            None,
            PeerRpcWireErrorCodeV2::Forbidden,
        );
        let output = format!("{error:?}");
        assert!(!output.contains("private-request-marker"));
        assert!(!output.contains("peer authorization failed"));
    }
}
