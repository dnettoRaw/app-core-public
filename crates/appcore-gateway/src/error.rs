// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/26 08:53:09 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 08:53:09 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Gateway-scoped error types.

use appcore_types::RuntimeError;

/// Failures produced during Gateway operation.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    /// A configuration validation or parsing failure.
    #[error("configuration error: {0}")]
    Config(String),

    /// Missing or cryptographically invalid token credentials.
    #[error("authentication failed: {0}")]
    Authentication(String),

    /// Valid credentials but unauthorized for the target resource.
    #[error("authorization failed: {0}")]
    Forbidden(String),

    /// Multi-tenant boundary checks failed.
    #[error("tenant mismatch: {0}")]
    TenantMismatch(String),

    /// No active worker is registered to serve the requested capability.
    #[error("worker unavailable for capability: {0}")]
    WorkerUnavailable(String),

    /// Underlying WebSocket connection transport failed.
    #[error("transport error: {0}")]
    Transport(String),

    /// JSON serialization or deserialization failed.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Protocol or envelope structure violation.
    #[error("protocol violation: {0}")]
    Protocol(String),

    /// Inner AppCore runtime error.
    #[error("runtime core error: {0:?}")]
    Runtime(RuntimeError),
}

impl From<RuntimeError> for GatewayError {
    fn from(err: RuntimeError) -> Self {
        GatewayError::Runtime(err)
    }
}

/// Specialized Result type for Gateway operations.
pub type GatewayResult<T> = Result<T, GatewayError>;
