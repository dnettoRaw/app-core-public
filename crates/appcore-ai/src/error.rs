// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/21 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/21 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

use crate::BackendId;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Result returned by AppCore AI operations.
pub type AiResult<T> = Result<T, AiError>;

/// A bounded contract dimension that was exceeded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LimitKind {
    /// Total encoded request input bytes.
    InputBytes,
    /// Total encoded response output bytes.
    OutputBytes,
    /// Number of request input parts.
    InputParts,
    /// Number of response metadata entries.
    MetadataEntries,
    /// Encoded metadata key length.
    MetadataKeyBytes,
    /// Encoded metadata value length.
    MetadataValueBytes,
    /// Number of routing or escalation attempts.
    Attempts,
    /// Number of peers considered for distributed work.
    Peers,
}

/// Stable failure categories returned by `appcore-ai`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AiError {
    /// A validated identifier or request field is invalid.
    InvalidInput(&'static str),
    /// A configured bound was exceeded.
    LimitExceeded {
        /// The exceeded dimension.
        kind: LimitKind,
        /// Observed value.
        actual: u64,
        /// Maximum accepted value.
        limit: u64,
    },
    /// Cooperative cancellation was observed.
    Cancelled,
    /// The request deadline elapsed before completion.
    DeadlineExceeded,
    /// A required model, backend, device, artifact or route was not found.
    NotFound(&'static str),
    /// A registration conflicts with existing state.
    Conflict(&'static str),
    /// Available resources cannot safely admit the request.
    Capacity(&'static str),
    /// A bounded queue cannot accept more work.
    QueueFull,
    /// A selected backend is unavailable or unhealthy.
    BackendUnavailable(BackendId),
    /// A backend failed without exposing its private diagnostic text.
    BackendFailure {
        /// Backend that reported the failure.
        backend: BackendId,
        /// Stable backend-neutral reason code.
        code: &'static str,
    },
    /// The requested model, device or backend combination is incompatible.
    Incompatible(&'static str),
    /// Artifact identity, digest or provenance validation failed.
    Integrity(&'static str),
    /// An authenticated identity lacks permission for the operation.
    Unauthorized,
    /// Swarm execution was requested without an available authorized bridge.
    SwarmUnavailable,
    /// The requested capability is not implemented by the selected path.
    Unsupported(&'static str),
    /// A bounded synchronization primitive was poisoned.
    InternalState,
}

impl Display for AiError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(field) => write!(formatter, "invalid AI input: {field}"),
            Self::LimitExceeded {
                kind,
                actual,
                limit,
            } => write!(
                formatter,
                "AI limit exceeded for {kind:?}: actual={actual}, limit={limit}"
            ),
            Self::Cancelled => formatter.write_str("AI operation was cancelled"),
            Self::DeadlineExceeded => formatter.write_str("AI operation deadline exceeded"),
            Self::NotFound(owner) => write!(formatter, "AI resource was not found: {owner}"),
            Self::Conflict(owner) => write!(formatter, "AI registration conflict: {owner}"),
            Self::Capacity(reason) => write!(formatter, "AI capacity unavailable: {reason}"),
            Self::QueueFull => formatter.write_str("AI queue is full"),
            Self::BackendUnavailable(backend) => {
                write!(formatter, "AI backend is unavailable: {backend}")
            }
            Self::BackendFailure { backend, code } => {
                write!(
                    formatter,
                    "AI backend failed: backend={backend}, code={code}"
                )
            }
            Self::Incompatible(reason) => write!(formatter, "incompatible AI route: {reason}"),
            Self::Integrity(reason) => {
                write!(formatter, "AI integrity validation failed: {reason}")
            }
            Self::Unauthorized => formatter.write_str("AI operation is unauthorized"),
            Self::SwarmUnavailable => formatter.write_str("AI swarm is unavailable"),
            Self::Unsupported(reason) => write!(formatter, "unsupported AI operation: {reason}"),
            Self::InternalState => formatter.write_str("AI internal state is unavailable"),
        }
    }
}

impl Error for AiError {}
