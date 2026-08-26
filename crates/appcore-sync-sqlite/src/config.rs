// =============================================================================
//        #######
//     ###       ###     F: config.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::{SqliteSyncError, SqliteSyncResult};
use std::fmt;
use std::path::{Path, PathBuf};

const MIB: u64 = 1024 * 1024;

/// Bounded configuration for one SQLite sync database.
#[derive(Clone, PartialEq, Eq)]
pub struct SqliteSyncConfig {
    pub(crate) path: PathBuf,
    pub(crate) max_database_bytes: u64,
    pub(crate) max_outbox_entries: usize,
    pub(crate) max_checkpoints: usize,
    pub(crate) max_outbox_record_bytes: usize,
    pub(crate) max_read_records: usize,
    pub(crate) max_read_bytes: usize,
    pub(crate) max_tombstones: usize,
    pub(crate) max_connections: usize,
    pub(crate) busy_timeout_ms: u64,
    pub(crate) backup_pages_per_step: i32,
}

impl SqliteSyncConfig {
    /// Creates a configuration with conservative production bounds.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            max_database_bytes: 512 * MIB,
            max_outbox_entries: 10_000,
            max_checkpoints: 10_000,
            max_outbox_record_bytes: 48 * MIB as usize,
            max_read_records: 4_096,
            max_read_bytes: 16 * MIB as usize,
            max_tombstones: 100_000,
            max_connections: 8,
            busy_timeout_ms: 5_000,
            backup_pages_per_step: 128,
        }
    }

    /// Selects the maximum database size in bytes.
    pub fn with_max_database_bytes(mut self, value: u64) -> Self {
        self.max_database_bytes = value;
        self
    }

    /// Selects the maximum number of pending outbox entries.
    pub fn with_max_outbox_entries(mut self, value: usize) -> Self {
        self.max_outbox_entries = value;
        self
    }

    /// Selects the maximum number of peer checkpoints.
    pub fn with_max_checkpoints(mut self, value: usize) -> Self {
        self.max_checkpoints = value;
        self
    }

    /// Selects the maximum encoded size of one outbox entry.
    pub fn with_max_outbox_record_bytes(mut self, value: usize) -> Self {
        self.max_outbox_record_bytes = value;
        self
    }

    /// Selects the maximum record count returned by one log read.
    pub fn with_max_read_records(mut self, value: usize) -> Self {
        self.max_read_records = value;
        self
    }

    /// Selects the maximum payload bytes returned by one log read.
    pub fn with_max_read_bytes(mut self, value: usize) -> Self {
        self.max_read_bytes = value;
        self
    }

    /// Selects the maximum retained tombstone count.
    pub fn with_max_tombstones(mut self, value: usize) -> Self {
        self.max_tombstones = value;
        self
    }

    /// Selects the maximum number of simultaneously open SQLite connections.
    pub fn with_max_connections(mut self, value: usize) -> Self {
        self.max_connections = value;
        self
    }

    /// Selects the SQLite writer-admission timeout in milliseconds.
    pub fn with_busy_timeout_ms(mut self, value: u64) -> Self {
        self.busy_timeout_ms = value;
        self
    }

    /// Selects the number of pages copied by each online-backup step.
    pub fn with_backup_pages_per_step(mut self, value: i32) -> Self {
        self.backup_pages_per_step = value;
        self
    }

    /// Returns the configured database path.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn validate(&self) -> SqliteSyncResult<()> {
        if self.path.as_os_str().is_empty() {
            return Err(SqliteSyncError::InvalidConfiguration("empty path"));
        }
        if !(8 * MIB..=8 * 1024 * MIB).contains(&self.max_database_bytes) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "database byte bound is outside 8 MiB..=8 GiB",
            ));
        }
        if !(1..=100_000).contains(&self.max_outbox_entries) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "outbox entry bound is outside 1..=100000",
            ));
        }
        if !(1..=100_000).contains(&self.max_checkpoints) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "checkpoint bound is outside 1..=100000",
            ));
        }
        if !(MIB as usize..=48 * MIB as usize).contains(&self.max_outbox_record_bytes) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "outbox record bound is outside 1 MiB..=48 MiB",
            ));
        }
        if !(1..=10_000).contains(&self.max_read_records) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "read record bound is outside 1..=10000",
            ));
        }
        if !(MIB as usize..=64 * MIB as usize).contains(&self.max_read_bytes) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "read byte bound is outside 1 MiB..=64 MiB",
            ));
        }
        if !(1..=1_000_000).contains(&self.max_tombstones) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "tombstone bound is outside 1..=1000000",
            ));
        }
        if !(1..=32).contains(&self.max_connections) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "connection bound is outside 1..=32",
            ));
        }
        if !(1..=60_000).contains(&self.busy_timeout_ms) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "busy timeout is outside 1..=60000 ms",
            ));
        }
        if !(1..=4_096).contains(&self.backup_pages_per_step) {
            return Err(SqliteSyncError::InvalidConfiguration(
                "backup step is outside 1..=4096 pages",
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for SqliteSyncConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteSyncConfig")
            .field("path_configured", &!self.path.as_os_str().is_empty())
            .field("max_database_bytes", &self.max_database_bytes)
            .field("max_outbox_entries", &self.max_outbox_entries)
            .field("max_checkpoints", &self.max_checkpoints)
            .field("max_outbox_record_bytes", &self.max_outbox_record_bytes)
            .field("max_read_records", &self.max_read_records)
            .field("max_read_bytes", &self.max_read_bytes)
            .field("max_tombstones", &self.max_tombstones)
            .field("max_connections", &self.max_connections)
            .field("busy_timeout_ms", &self.busy_timeout_ms)
            .field("backup_pages_per_step", &self.backup_pages_per_step)
            .finish()
    }
}
