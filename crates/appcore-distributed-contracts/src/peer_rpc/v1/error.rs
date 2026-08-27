// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0-beta.1
// =============================================================================

//! Exact decoding of controlled rejection strings from frozen Peer RPC V1.

use std::fmt::{Debug, Display, Formatter};

/// Exact controlled error codes emitted by a Peer RPC V1 host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerRpcRemoteErrorCodeV1 {
    /// The request exceeds a configured bound.
    PayloadTooLarge,
    /// The credential is missing or invalid.
    Unauthorized,
    /// The credential does not authorize this request.
    Forbidden,
    /// The selected endpoint cannot currently accept work.
    EndpointUnavailable,
    /// Tenant isolation differs from the target.
    TenantMismatch,
    /// Cluster isolation differs from the target.
    ClusterMismatch,
    /// The envelope names another target Core.
    TargetMismatch,
    /// The selected protocol is incompatible.
    ProtocolMismatch,
    /// The signed envelope expired.
    Expired,
    /// The nonce was already accepted.
    NonceReplay,
    /// Replay admission reached its configured capacity.
    NonceCacheFull,
    /// Payload integrity validation failed.
    InvalidBodyHash,
    /// The host observed an invalid response.
    InvalidResponse,
    /// The host transport failed.
    Transport,
    /// The request envelope is invalid.
    InvalidEnvelope,
    /// The rejection was absent or is not part of the frozen V1 matrix.
    Unknown,
}

impl PeerRpcRemoteErrorCodeV1 {
    /// Decodes one V1 rejection by exact equality only.
    pub fn decode(value: Option<&str>) -> Self {
        match value {
            Some("payload_too_large") => Self::PayloadTooLarge,
            Some("unauthorized") => Self::Unauthorized,
            Some("forbidden") => Self::Forbidden,
            Some("endpoint_unavailable") => Self::EndpointUnavailable,
            Some("tenant_mismatch") => Self::TenantMismatch,
            Some("cluster_mismatch") => Self::ClusterMismatch,
            Some("target_mismatch") => Self::TargetMismatch,
            Some("protocol_mismatch") => Self::ProtocolMismatch,
            Some("expired") => Self::Expired,
            Some("nonce_replay") => Self::NonceReplay,
            Some("nonce_cache_full") => Self::NonceCacheFull,
            Some("invalid_body_hash") => Self::InvalidBodyHash,
            Some("invalid_response") => Self::InvalidResponse,
            Some("transport") => Self::Transport,
            Some("invalid_envelope") => Self::InvalidEnvelope,
            _ => Self::Unknown,
        }
    }

    /// Reports whether the frozen V1 matrix permits a bounded retry.
    pub const fn retryable(self) -> bool {
        matches!(self, Self::EndpointUnavailable | Self::NonceCacheFull)
    }

    /// Returns the stable controlled spelling without retaining remote input.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PayloadTooLarge => "payload_too_large",
            Self::Unauthorized => "unauthorized",
            Self::Forbidden => "forbidden",
            Self::EndpointUnavailable => "endpoint_unavailable",
            Self::TenantMismatch => "tenant_mismatch",
            Self::ClusterMismatch => "cluster_mismatch",
            Self::TargetMismatch => "target_mismatch",
            Self::ProtocolMismatch => "protocol_mismatch",
            Self::Expired => "expired",
            Self::NonceReplay => "nonce_replay",
            Self::NonceCacheFull => "nonce_cache_full",
            Self::InvalidBodyHash => "invalid_body_hash",
            Self::InvalidResponse => "invalid_response",
            Self::Transport => "transport",
            Self::InvalidEnvelope => "invalid_envelope",
            Self::Unknown => "unknown",
        }
    }
}

impl Display for PeerRpcRemoteErrorCodeV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Typed local representation of one frozen Peer RPC V1 rejection.
#[derive(Clone, PartialEq, Eq)]
pub struct PeerRpcRemoteErrorV1 {
    code: PeerRpcRemoteErrorCodeV1,
    correlation_id: String,
}

impl PeerRpcRemoteErrorV1 {
    /// Decodes a V1 rejection without retaining unknown remote text.
    pub fn decode(error: Option<&str>, correlation_id: impl Into<String>) -> Self {
        Self {
            code: PeerRpcRemoteErrorCodeV1::decode(error),
            correlation_id: correlation_id.into(),
        }
    }

    /// Returns the exact known code or the bounded `unknown` outcome.
    pub const fn code(&self) -> PeerRpcRemoteErrorCodeV1 {
        self.code
    }

    /// Returns the response correlation identity checked by the client.
    pub fn correlation_id(&self) -> &str {
        &self.correlation_id
    }

    /// Reports whether a bounded retry is allowed by the V1 matrix.
    pub const fn retryable(&self) -> bool {
        self.code.retryable()
    }
}

impl Debug for PeerRpcRemoteErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PeerRpcRemoteErrorV1")
            .field("code", &self.code)
            .field("has_correlation_id", &!self.correlation_id.is_empty())
            .finish()
    }
}

impl Display for PeerRpcRemoteErrorV1 {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "remote peer rejected request: {}", self.code)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_decoder_uses_exact_codes_and_discards_unknown_input() {
        let known = [
            (
                "payload_too_large",
                PeerRpcRemoteErrorCodeV1::PayloadTooLarge,
            ),
            ("unauthorized", PeerRpcRemoteErrorCodeV1::Unauthorized),
            ("forbidden", PeerRpcRemoteErrorCodeV1::Forbidden),
            (
                "endpoint_unavailable",
                PeerRpcRemoteErrorCodeV1::EndpointUnavailable,
            ),
            ("tenant_mismatch", PeerRpcRemoteErrorCodeV1::TenantMismatch),
            (
                "cluster_mismatch",
                PeerRpcRemoteErrorCodeV1::ClusterMismatch,
            ),
            ("target_mismatch", PeerRpcRemoteErrorCodeV1::TargetMismatch),
            (
                "protocol_mismatch",
                PeerRpcRemoteErrorCodeV1::ProtocolMismatch,
            ),
            ("expired", PeerRpcRemoteErrorCodeV1::Expired),
            ("nonce_replay", PeerRpcRemoteErrorCodeV1::NonceReplay),
            ("nonce_cache_full", PeerRpcRemoteErrorCodeV1::NonceCacheFull),
            (
                "invalid_body_hash",
                PeerRpcRemoteErrorCodeV1::InvalidBodyHash,
            ),
            (
                "invalid_response",
                PeerRpcRemoteErrorCodeV1::InvalidResponse,
            ),
            ("transport", PeerRpcRemoteErrorCodeV1::Transport),
            (
                "invalid_envelope",
                PeerRpcRemoteErrorCodeV1::InvalidEnvelope,
            ),
        ];
        for (encoded, expected) in known {
            assert_eq!(PeerRpcRemoteErrorCodeV1::decode(Some(encoded)), expected);
        }
        assert_eq!(
            PeerRpcRemoteErrorCodeV1::decode(Some("prefix_endpoint_unavailable_suffix")),
            PeerRpcRemoteErrorCodeV1::Unknown
        );
        assert_eq!(
            PeerRpcRemoteErrorCodeV1::decode(None),
            PeerRpcRemoteErrorCodeV1::Unknown
        );
    }

    #[test]
    fn only_capacity_and_availability_are_retryable_in_v1() {
        assert!(PeerRpcRemoteErrorCodeV1::EndpointUnavailable.retryable());
        assert!(PeerRpcRemoteErrorCodeV1::NonceCacheFull.retryable());
        assert!(!PeerRpcRemoteErrorCodeV1::Unknown.retryable());
        assert!(!PeerRpcRemoteErrorCodeV1::Transport.retryable());
    }

    #[test]
    fn debug_omits_correlation_identity() {
        let error = PeerRpcRemoteErrorV1::decode(Some("forbidden"), "private-request-marker");
        assert!(!format!("{error:?}").contains("private-request-marker"));
    }
}
