// =============================================================================
//        #######
//     ###       ###     F: store.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

use crate::integrity::validate_internal_records;
use crate::schema;
use crate::{
    SqliteReplicationLog, SqliteSyncCheckpointStore, SqliteSyncConfig, SqliteSyncError,
    SqliteSyncOutbox, SqliteSyncResult, SqliteSyncTombstoneStore,
};
use appcore_contracts::ProviderId;
use appcore_storage::{
    StorageCapabilityDescriptorV1, StorageCapabilityProviderV1, StorageCapabilityV1,
};
use parking_lot::{Condvar, Mutex};
use rusqlite::limits::Limit;
use rusqlite::{Connection, OpenFlags};
use std::fmt;
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Result of a provider integrity inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqliteSyncHealth {
    /// Internal schema version observed by the provider.
    pub schema_version: u32,
    /// Database pages currently allocated.
    pub page_count: u64,
    /// Configured maximum page count.
    pub max_page_count: u64,
}

struct SqliteSyncInner {
    pool: Mutex<ConnectionPool>,
    available: Condvar,
    config: SqliteSyncConfig,
}

struct ConnectionPool {
    idle: Vec<Connection>,
    total: usize,
}

struct ConnectionGuard<'a> {
    inner: &'a SqliteSyncInner,
    connection: Option<Connection>,
}

/// Shared owner of one bounded SQLite sync database.
#[derive(Clone)]
pub struct SqliteSyncStore {
    inner: Arc<SqliteSyncInner>,
}

impl SqliteSyncStore {
    /// Opens, migrates and integrity-checks one provider database.
    pub fn open(mut config: SqliteSyncConfig) -> SqliteSyncResult<Self> {
        config.validate()?;
        config.path = normalize_path(&config.path)?;
        let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_FULL_MUTEX;
        let mut connection =
            Connection::open_with_flags(&config.path, flags).map_err(SqliteSyncError::database)?;
        configure(&connection, &config)?;
        schema::migrate(&mut connection)?;
        integrity_check(&connection, &config)?;
        Ok(Self {
            inner: Arc::new(SqliteSyncInner {
                pool: Mutex::new(ConnectionPool {
                    idle: vec![connection],
                    total: 1,
                }),
                available: Condvar::new(),
                config,
            }),
        })
    }

    /// Runs an integrity check and reports bounded database usage.
    pub fn health(&self) -> SqliteSyncResult<SqliteSyncHealth> {
        self.with_connection(|connection| {
            integrity_check(connection, &self.inner.config)?;
            let schema_version = connection
                .pragma_query_value(None, "user_version", |row| row.get(0))
                .map_err(SqliteSyncError::database)?;
            let page_count: i64 = connection
                .pragma_query_value(None, "page_count", |row| row.get(0))
                .map_err(SqliteSyncError::database)?;
            let max_page_count: i64 = connection
                .pragma_query_value(None, "max_page_count", |row| row.get(0))
                .map_err(SqliteSyncError::database)?;
            Ok(SqliteSyncHealth {
                schema_version,
                page_count: u64::try_from(page_count)
                    .map_err(|_| SqliteSyncError::CorruptRecord("page count"))?,
                max_page_count: u64::try_from(max_page_count)
                    .map_err(|_| SqliteSyncError::CorruptRecord("page limit"))?,
            })
        })
    }

    /// Returns the provider's redacted configuration bounds.
    pub fn config(&self) -> &SqliteSyncConfig {
        &self.inner.config
    }

    /// Creates a replication-log handle backed by this database.
    pub fn replication_log(&self) -> SqliteReplicationLog {
        SqliteReplicationLog::new(self.clone())
    }

    /// Creates a checkpoint-store handle backed by this database.
    pub fn checkpoint_store(&self) -> SqliteSyncCheckpointStore {
        SqliteSyncCheckpointStore::new(self.clone())
    }

    /// Creates an outbox handle backed by this database.
    pub fn outbox(&self) -> SqliteSyncOutbox {
        SqliteSyncOutbox::new(self.clone())
    }

    /// Creates an opaque tombstone-store handle backed by this database.
    pub fn tombstone_store(&self) -> SqliteSyncTombstoneStore {
        SqliteSyncTombstoneStore::new(self.clone())
    }

    pub(crate) fn with_connection<T>(
        &self,
        action: impl FnOnce(&mut Connection) -> SqliteSyncResult<T>,
    ) -> SqliteSyncResult<T> {
        let mut connection = self.acquire_connection()?;
        action(connection.connection_mut()?)
    }

    fn acquire_connection(&self) -> SqliteSyncResult<ConnectionGuard<'_>> {
        let deadline = Instant::now() + Duration::from_millis(self.inner.config.busy_timeout_ms);
        let mut pool = self.inner.pool.lock();
        loop {
            if let Some(connection) = pool.idle.pop() {
                return Ok(ConnectionGuard {
                    inner: &self.inner,
                    connection: Some(connection),
                });
            }
            if pool.total < self.inner.config.max_connections {
                pool.total += 1;
                drop(pool);
                return match open_connection(&self.inner.config) {
                    Ok(connection) => Ok(ConnectionGuard {
                        inner: &self.inner,
                        connection: Some(connection),
                    }),
                    Err(error) => {
                        let mut pool = self.inner.pool.lock();
                        pool.total = pool.total.saturating_sub(1);
                        self.inner.available.notify_one();
                        Err(error)
                    }
                };
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(SqliteSyncError::CapacityExceeded("connection"));
            }
            self.inner.available.wait_for(&mut pool, deadline - now);
        }
    }
}

impl ConnectionGuard<'_> {
    fn connection_mut(&mut self) -> SqliteSyncResult<&mut Connection> {
        self.connection
            .as_mut()
            .ok_or(SqliteSyncError::DatabaseOperation)
    }
}

impl Drop for ConnectionGuard<'_> {
    fn drop(&mut self) {
        if let Some(connection) = self.connection.take() {
            self.inner.pool.lock().idle.push(connection);
            self.inner.available.notify_one();
        }
    }
}

impl fmt::Debug for SqliteSyncStore {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SqliteSyncStore")
            .field("config", &self.inner.config)
            .finish_non_exhaustive()
    }
}

impl StorageCapabilityProviderV1 for SqliteSyncStore {
    fn storage_capabilities_v1(
        &self,
    ) -> Result<StorageCapabilityDescriptorV1, appcore_storage::StorageCapabilityError> {
        sqlite_sync_capability_descriptor_v1()
    }
}

/// Returns the conservative provider-independent guarantees for SQLite sync.
pub fn sqlite_sync_capability_descriptor_v1(
) -> Result<StorageCapabilityDescriptorV1, appcore_storage::StorageCapabilityError> {
    let provider_id = ProviderId::new("sqlite-sync")
        .map_err(|_| appcore_storage::StorageCapabilityError::InvalidDescriptor)?;
    Ok(StorageCapabilityDescriptorV1::new(
        provider_id,
        [
            StorageCapabilityV1::Transactions,
            StorageCapabilityV1::Locking,
            StorageCapabilityV1::Snapshot,
            StorageCapabilityV1::OnlineBackup,
            StorageCapabilityV1::MultiProcess,
        ],
    ))
}

fn configure(connection: &Connection, config: &SqliteSyncConfig) -> SqliteSyncResult<()> {
    connection
        .busy_timeout(Duration::from_millis(config.busy_timeout_ms))
        .map_err(SqliteSyncError::database)?;
    connection
        .pragma_update_and_check(None, "journal_mode", "WAL", |row| row.get::<_, String>(0))
        .map_err(SqliteSyncError::database)
        .and_then(|mode| {
            if mode.eq_ignore_ascii_case("wal") {
                Ok(())
            } else {
                Err(SqliteSyncError::DatabaseOperation)
            }
        })?;
    connection
        .pragma_update(None, "synchronous", "FULL")
        .and_then(|_| connection.pragma_update(None, "foreign_keys", true))
        .and_then(|_| connection.pragma_update(None, "trusted_schema", false))
        .and_then(|_| connection.pragma_update(None, "wal_autocheckpoint", 1_000))
        .map_err(SqliteSyncError::database)?;
    let max_length = i32::try_from(config.max_outbox_record_bytes)
        .map_err(|_| SqliteSyncError::InvalidConfiguration("outbox record bound overflow"))?;
    connection
        .set_limit(Limit::SQLITE_LIMIT_LENGTH, max_length)
        .and_then(|_| connection.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0))
        .and_then(|_| connection.set_limit(Limit::SQLITE_LIMIT_WORKER_THREADS, 0))
        .map_err(SqliteSyncError::database)?;
    let page_bytes: i64 = connection
        .pragma_query_value(None, "page_size", |row| row.get(0))
        .map_err(SqliteSyncError::database)?;
    let page_bytes =
        u64::try_from(page_bytes).map_err(|_| SqliteSyncError::CorruptRecord("page size"))?;
    if page_bytes == 0 {
        return Err(SqliteSyncError::CorruptRecord("page size"));
    }
    let max_pages = i64::try_from(config.max_database_bytes / page_bytes)
        .map_err(|_| SqliteSyncError::InvalidConfiguration("database page bound overflow"))?;
    connection
        .pragma_update(None, "max_page_count", max_pages)
        .map_err(SqliteSyncError::database)
}

fn open_connection(config: &SqliteSyncConfig) -> SqliteSyncResult<Connection> {
    let flags = OpenFlags::SQLITE_OPEN_READ_WRITE
        | OpenFlags::SQLITE_OPEN_CREATE
        | OpenFlags::SQLITE_OPEN_FULL_MUTEX;
    let connection =
        Connection::open_with_flags(&config.path, flags).map_err(SqliteSyncError::database)?;
    configure(&connection, config)?;
    Ok(connection)
}

fn integrity_check(connection: &Connection, config: &SqliteSyncConfig) -> SqliteSyncResult<()> {
    let result: String = connection
        .query_row("PRAGMA quick_check(1)", [], |row| row.get(0))
        .map_err(SqliteSyncError::database)?;
    if result != "ok" {
        return Err(SqliteSyncError::IntegrityFailed);
    }
    validate_internal_records(connection, config)
}

pub(crate) fn normalize_path(path: &Path) -> SqliteSyncResult<std::path::PathBuf> {
    let file_name = path.file_name().ok_or(SqliteSyncError::UnsafePath)?;
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_file() => {
            return Err(SqliteSyncError::UnsafePath);
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(SqliteSyncError::UnsafePath),
    }
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|_| SqliteSyncError::UnsafePath)?;
    let canonical_parent = fs::canonicalize(parent).map_err(|_| SqliteSyncError::UnsafePath)?;
    if !canonical_parent.is_dir() {
        return Err(SqliteSyncError::UnsafePath);
    }
    Ok(canonical_parent.join(file_name))
}
