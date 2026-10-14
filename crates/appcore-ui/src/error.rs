// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/13 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/13 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Controlled UI failures without free-form provider details or opaque payloads.
use serde::{Deserialize, Serialize};
use std::fmt;
/// Result at a UI infrastructure boundary.
pub type UiResult<T> = Result<T, UiError>;
/// Stable wire error codes; errors never contain credentials or application data.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum UiError {
    /// An unsupported protocol input was presented.
    UnsupportedVersion,
    /// The JSON frame is malformed.
    InvalidFrame,
    /// An identifier or operation violates the contract.
    InvalidRequest,
    /// Encoded or decoded data exceeds the selected budget.
    PayloadTooLarge,
    /// A command omitted its validated idempotency key.
    IdempotencyRequired,
    /// An equivalent command is still executing or requires effect reconciliation.
    IdempotencyPending,
    /// A key was reused for a semantically different operation.
    IdempotencyConflict,
    /// A valid authenticated principal is required.
    AuthenticationRequired,
    /// The owning authorization policy denies this operation.
    Forbidden,
    /// Admission or subscription capacity is exhausted.
    Busy,
    /// The selected transport/backend is unavailable.
    Unavailable,
    /// Cooperative cancellation was requested.
    Cancelled,
    /// The monotonic execution budget elapsed.
    DeadlineExceeded,
    /// The surface no longer admits operations.
    Closed,
    /// A subscription no longer exists in this surface generation.
    SubscriptionNotFound,
    /// A sequence cannot safely advance without identity reuse.
    SequenceExhausted,
    /// An owning backend failed; its raw detail remains outside the wire response.
    BackendFailure,
}
impl fmt::Display for UiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if *self == Self::UnsupportedVersion {
            return f.write_str("NO MORE SUPPORTED PLEASE UPDATE");
        }
        f.write_str(match self {
            Self::UnsupportedVersion => "NO MORE SUPPORTED PLEASE UPDATE",
            Self::InvalidFrame => "invalid UI frame",
            Self::InvalidRequest => "invalid UI request",
            Self::PayloadTooLarge => "UI payload limit exceeded",
            Self::IdempotencyRequired => "UI command requires idempotency",
            Self::IdempotencyPending => "UI command completion pending",
            Self::IdempotencyConflict => "UI command idempotency conflict",
            Self::AuthenticationRequired => "UI authentication required",
            Self::Forbidden => "UI operation forbidden",
            Self::Busy => "UI capacity exhausted",
            Self::Unavailable => "UI transport unavailable",
            Self::Cancelled => "UI operation cancelled",
            Self::DeadlineExceeded => "UI deadline exceeded",
            Self::Closed => "UI surface closed",
            Self::SubscriptionNotFound => "UI subscription not found",
            Self::SequenceExhausted => "UI sequence exhausted",
            Self::BackendFailure => "UI backend failure",
        })
    }
}
impl std::error::Error for UiError {}
