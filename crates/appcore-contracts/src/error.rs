// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/21 23:21:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Errors returned while constructing or decoding contracts.

use std::fmt::{Display, Formatter};

/// Result type used by AppCore contracts.
pub type ContractResult<T> = Result<T, ContractError>;

/// Validation error for a versioned contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// A required value was empty.
    Empty {
        /// Name of the invalid field.
        field: &'static str,
    },
    /// A value exceeded its documented maximum length.
    TooLong {
        /// Name of the invalid field.
        field: &'static str,
        /// Maximum accepted UTF-8 byte length.
        max_bytes: usize,
    },
    /// An identifier contains unsupported characters or delimiters.
    InvalidIdentifier {
        /// Name of the invalid identifier field.
        field: &'static str,
    },
    /// A field combination is not valid.
    InvalidValue {
        /// Name of the invalid field.
        field: &'static str,
        /// Stable, non-sensitive validation reason.
        reason: &'static str,
    },
    /// A collection contains the same logical key more than once.
    Duplicate {
        /// Name of the collection field.
        field: &'static str,
        /// Repeated non-sensitive identifier.
        value: String,
    },
    /// A manifest attempted to store a secret instead of a secret reference.
    SecretValue {
        /// Field that must be replaced by a secret reference.
        field: String,
    },
    /// An application manifest attempted to store an installation-local path.
    LocalPath {
        /// Application-owned field that attempted to carry a path.
        field: String,
    },
}

impl Display for ContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty { field } => write!(formatter, "{field} must not be empty"),
            Self::TooLong { field, max_bytes } => {
                write!(formatter, "{field} must not exceed {max_bytes} bytes")
            }
            Self::InvalidIdentifier { field } => {
                write!(formatter, "{field} is not a valid distributed identifier")
            }
            Self::InvalidValue { field, reason } => write!(formatter, "{field}: {reason}"),
            Self::Duplicate { field, value } => write!(formatter, "duplicate {field}: {value}"),
            Self::SecretValue { field } => {
                write!(formatter, "{field} must use a secret reference")
            }
            Self::LocalPath { field } => {
                write!(formatter, "{field} must be declared by the deployment")
            }
        }
    }
}

impl std::error::Error for ContractError {}
