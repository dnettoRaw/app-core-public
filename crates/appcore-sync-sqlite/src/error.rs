// =============================================================================
//        #######
//     ###       ###     F: error.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use appcore_sync::SyncError;
use std::fmt;

/// Result produced by the SQLite sync provider.
pub type SqliteSyncResult<T> = Result<T, SqliteSyncError>;

/// Typed provider error whose diagnostics never contain paths, SQL or payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqliteSyncError {
    /// Provider configuration violates a fixed bound.
    InvalidConfiguration(&'static str),
    /// The database path is not an owner-controlled regular-file location.
    UnsafePath,
    /// The persistent schema is removed, unversioned or newer than supported.
    UpdateRequired,
    /// SQLite rejected an internal operation.
    DatabaseOperation,
    /// Database integrity validation failed.
    IntegrityFailed,
    /// A provider capacity limit was reached.
    CapacityExceeded(&'static str),
    /// A stored provider record is structurally invalid.
    CorruptRecord(&'static str),
}

impl SqliteSyncError {
    pub(crate) fn database(_error: rusqlite::Error) -> Self {
        Self::DatabaseOperation
    }

    pub(crate) fn sync(self) -> SyncError {
        SyncError::ReplicationFailed(self.to_string())
    }
}

impl fmt::Display for SqliteSyncError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfiguration(reason) => {
                write!(formatter, "invalid SQLite sync configuration: {reason}")
            }
            Self::UnsafePath => formatter.write_str("SQLite sync path is unsafe"),
            Self::UpdateRequired => formatter.write_str("NO MORE SUPPORTED PLEASE UPDATE"),
            Self::DatabaseOperation => formatter.write_str("SQLite sync operation failed"),
            Self::IntegrityFailed => formatter.write_str("SQLite sync integrity check failed"),
            Self::CapacityExceeded(resource) => {
                write!(formatter, "SQLite sync {resource} capacity exceeded")
            }
            Self::CorruptRecord(kind) => write!(formatter, "corrupt SQLite sync {kind} record"),
        }
    }
}

impl std::error::Error for SqliteSyncError {}
