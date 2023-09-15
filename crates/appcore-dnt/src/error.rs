// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/02 00:04:12 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 10:29:16 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! DNT error contracts.

/// DNT-local result type.
pub type DntResult<T> = Result<T, DntError>;

/// Controlled DNT failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DntError {
    /// The bytes do not contain a structurally valid DNT envelope.
    #[error("invalid DNT container")]
    InvalidFormat,
    /// The envelope version is newer or otherwise unsupported.
    #[error("unsupported DNT envelope version")]
    UnsupportedVersion,
    /// The configured maximum payload size would be exceeded.
    #[error("DNT payload exceeds configured limit")]
    PayloadTooLarge,
    /// The envelope contains an impossible or unsupported flag combination.
    #[error("DNT flags are invalid")]
    InvalidFlags,
    /// The envelope is not bound to the expected context.
    #[error("DNT context mismatch")]
    ContextMismatch,
    /// The key provider could not return usable material.
    #[error("DNT key is unavailable")]
    KeyUnavailable,
    /// AEAD authentication failed.
    #[error("DNT authentication failed")]
    AuthenticationFailed,
    /// The codec identifier is unknown or incompatible.
    #[error("DNT codec is unavailable")]
    CodecUnavailable,
    /// Encoding or decoding failed without exposing payload material.
    #[error("DNT codec failed")]
    CodecFailed,
    /// Filesystem persistence failed.
    #[error("DNT I/O failed")]
    Io,
}

/// Controlled key-provider failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DntKeyError {
    /// The requested key identity is unknown.
    #[error("DNT key was not found")]
    NotFound,
    /// The key provider is intentionally unavailable.
    #[error("DNT key provider is unavailable")]
    Unavailable,
    /// The resolved key material is malformed.
    #[error("DNT key material is invalid")]
    InvalidKey,
    /// The key is not valid for the supplied context.
    #[error("DNT key is not valid for the requested context")]
    Forbidden,
}

/// Controlled codec failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    /// The codec cannot encode the supplied bytes.
    #[error("DNT codec encode failed")]
    EncodeFailed,
    /// The codec cannot decode the supplied bytes.
    #[error("DNT codec decode failed")]
    DecodeFailed,
    /// The payload violates a codec-specific bound.
    #[error("DNT codec payload is too large")]
    PayloadTooLarge,
}

impl From<DntKeyError> for DntError {
    fn from(value: DntKeyError) -> Self {
        match value {
            DntKeyError::NotFound | DntKeyError::Unavailable => Self::KeyUnavailable,
            DntKeyError::InvalidKey => Self::KeyUnavailable,
            DntKeyError::Forbidden => Self::ContextMismatch,
        }
    }
}

impl From<CodecError> for DntError {
    fn from(_: CodecError) -> Self {
        Self::CodecFailed
    }
}
