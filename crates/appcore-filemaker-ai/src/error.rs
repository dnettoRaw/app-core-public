// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/30 05:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/30 05:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

/// Bridge failure that never exposes unrestricted tool payloads.
#[derive(Debug, thiserror::Error)]
pub enum BridgeError {
    /// Tool name or bounded JSON arguments are invalid.
    #[error("invalid FileMaker AI tool input: {0}")]
    InvalidInput(&'static str),
    /// A stateful tool was called before a document was loaded.
    #[error("FileMaker AI session has no document")]
    NoDocument,
    /// Explicit edit or resource policy rejected the call.
    #[error("FileMaker AI policy rejected the call: {0}")]
    Policy(String),
    /// Deterministic core failure.
    #[error(transparent)]
    Core(#[from] appcore_filemaker::FileMakerError),
    /// Bounded JSON conversion failure.
    #[error("FileMaker AI JSON conversion failed: {0}")]
    Json(String),
}

/// Result returned by bridge operations.
pub type BridgeResult<T> = Result<T, BridgeError>;

pub(crate) fn json_error(error: serde_json::Error) -> BridgeError {
    let mut message = error.to_string();
    message.truncate(512);
    BridgeError::Json(message)
}
