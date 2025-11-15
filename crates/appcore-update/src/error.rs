// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 13:45:20 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::UpdateFaultPoint;

/// Update lifecycle failure.
#[derive(Debug, thiserror::Error)]
pub enum UpdateError {
    /// Artifact metadata is invalid.
    #[error("invalid artifact metadata: {0}")]
    InvalidArtifact(String),
    /// Artifact is incompatible with the current runtime or protocol.
    #[error("incompatible artifact: {0}")]
    Incompatible(String),
    /// Provider failed to list or fetch an artifact.
    #[error("update provider failed: {0}")]
    Provider(String),
    /// Artifact exceeded the configured byte bound.
    #[error("artifact exceeds maximum size of {max_bytes} bytes")]
    ArtifactTooLarge {
        /// Configured maximum size.
        max_bytes: usize,
    },
    /// Artifact bytes do not match the declared checksum.
    #[error("artifact checksum mismatch")]
    ChecksumMismatch,
    /// Artifact authenticity could not be established.
    #[error("artifact authenticity verification failed: {0}")]
    Authenticity(String),
    /// Artifact storage failed.
    #[error("artifact store failed: {0}")]
    Store(String),
    /// Activation health verification failed.
    #[error("activated artifact failed health verification: {0}")]
    Health(String),
    /// A controlled fault was injected for validation.
    #[error("injected update fault at {0:?}")]
    InjectedFault(UpdateFaultPoint),
    /// Rollback failed after activation failed.
    #[error("rollback failed after {cause}: {rollback}")]
    RollbackFailed {
        /// Original activation or health failure.
        cause: String,
        /// Rollback failure detail.
        rollback: String,
    },
}

/// Result returned by the update lifecycle.
pub type UpdateResult<T> = Result<T, UpdateError>;
