// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Errors returned by `SDK` setup and zero-configuration manifest synthesis.

use appcore_contracts::ContractError;
use appcore_core::RuntimeError;
use appcore_log::LogConfigError;

/// Error returned by application-facing `SDK` operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppError {
    /// A canonical `AppCore` contract was invalid.
    Contract(ContractError),
    /// A Runtime identifier or application callback was rejected.
    Runtime(RuntimeError),
    /// Explicit manifests describe different applications.
    ManifestIdentityMismatch,
    /// Explicit logger destinations or limits were invalid.
    Logging(LogConfigError),
    /// A diagnostic bundle field or serialization operation was invalid.
    Diagnostics(String),
    /// A unified SDK environment profile was invalid.
    Environment(String),
    /// A capability declaration or command mapping was invalid.
    Capability(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract(error) => write!(formatter, "invalid AppCore contract: {error:?}"),
            Self::Runtime(error) => write!(formatter, "runtime error: {error:?}"),
            Self::ManifestIdentityMismatch => {
                formatter.write_str("application and deployment manifests identify different apps")
            }
            Self::Logging(error) => write!(formatter, "invalid logging configuration: {error:?}"),
            Self::Diagnostics(error) => write!(formatter, "invalid diagnostics: {error}"),
            Self::Environment(error) => write!(formatter, "invalid environment: {error}"),
            Self::Capability(error) => write!(formatter, "invalid capability: {error}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<ContractError> for AppError {
    fn from(error: ContractError) -> Self {
        Self::Contract(error)
    }
}

impl From<RuntimeError> for AppError {
    fn from(error: RuntimeError) -> Self {
        Self::Runtime(error)
    }
}

impl From<LogConfigError> for AppError {
    fn from(error: LogConfigError) -> Self {
        Self::Logging(error)
    }
}

/// Result used by application-facing `SDK` operations.
pub type AppResult<T> = Result<T, AppError>;
