// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/06 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/06 00:00:00 by dnettoRaw
//      ###########      S: 0.1.0-beta.1
// =============================================================================

//! Controlled errors without payloads, credentials or unbounded provider messages.
/// Controlled transfer failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TransferError {
    /// Policy or descriptor is malformed.
    #[error("invalid transfer contract")]
    InvalidContract,
    /// Unknown serialized format version.
    #[error("NO MORE SUPPORTED PLEASE UPDATE")]
    UnsupportedVersion,
    /// A resource budget would be exceeded.
    #[error("transfer resource limit exceeded")]
    Limit,
    /// Untrusted path, symlink or filesystem permissions.
    #[error("unsafe transfer path")]
    UnsafePath,
    /// Another process/session holds the store lock.
    #[error("transfer store busy")]
    Busy,
    /// Persistent data or received block failed integrity validation.
    #[error("transfer integrity mismatch")]
    Integrity,
    /// Filesystem operation failed; resumable progress may already be durable.
    #[error("transfer storage failure")]
    Storage,
    /// No authorized, usable source remains or bounded retries were exhausted.
    #[error("transfer sources exhausted")]
    SourcesExhausted,
    /// Cooperative cancellation requested.
    #[error("transfer cancelled")]
    Cancelled,
    /// Monotonic deadline expired.
    #[error("transfer deadline expired")]
    Deadline,
    /// Selected provider failed or panicked.
    #[error("transfer source failure")]
    Source,
}
/// Result from bounded transfer operations.
pub type TransferResult<T> = Result<T, TransferError>;
