// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Bounded SQLite persistence for Runtime-owned synchronization state.

#![deny(missing_docs)]

mod backup;
mod checkpoint;
mod config;
mod error;
mod integrity;
mod log;
mod outbox;
mod schema;
mod store;
mod tombstone;

pub use backup::SqliteBackupReport;
pub use checkpoint::SqliteSyncCheckpointStore;
pub use config::SqliteSyncConfig;
pub use error::{SqliteSyncError, SqliteSyncResult};
pub use log::SqliteReplicationLog;
pub use outbox::SqliteSyncOutbox;
pub use store::{sqlite_sync_capability_descriptor_v1, SqliteSyncHealth, SqliteSyncStore};
pub use tombstone::{SqliteSyncTombstone, SqliteSyncTombstoneStore};

/// Original internal SQLite schema version.
pub const SQLITE_SYNC_SCHEMA_V1: u32 = 1;
/// Current internal SQLite schema with bounded outbox retry metadata.
pub const SQLITE_SYNC_SCHEMA_V2: u32 = 2;
