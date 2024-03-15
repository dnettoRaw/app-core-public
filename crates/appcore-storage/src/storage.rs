// =============================================================================
//        #######
//     ###       ###     F: storage.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 00:04:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Minimal storage contracts (no concrete database implementation).

/// Storage-local result type.
pub type StorageResult<T> = Result<T, StorageError>;

/// Storage-local typed errors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// Provider is unavailable or not open.
    NotAvailable,
    /// Requested repository or file does not exist.
    RepositoryNotFound(String),
    /// Migration execution or compatibility failed.
    MigrationFailed(String),
    /// Atomic storage operation failed.
    TransactionFailed(String),
    /// Provider does not implement transaction semantics.
    TransactionsUnsupported,
    /// Backup operation failed.
    BackupFailed(String),
    /// Path escaped or violated the provider root policy.
    InvalidPath(String),
    /// Cryptographic storage operation failed.
    SecurityFailed(String),
    /// Required authentication boundary is unavailable.
    AuthUnavailable(String),
}

/// Coarse storage availability status.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageStatus {
    /// Provider is available for reads and writes.
    Online,
    /// Provider remains available with reduced guarantees.
    Degraded,
    /// Provider accepts reads only.
    ReadOnly,
    /// Provider is unavailable.
    Offline,
}

/// Storage health snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageHealth {
    /// Coarse storage status.
    pub status: StorageStatus,
    /// Optional non-sensitive detail.
    pub message: Option<String>,
}

/// Stable repository name.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RepositoryName(
    /// Stable repository identifier.
    pub String,
);

/// Stable migration identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MigrationId(
    /// Stable migration identifier.
    pub String,
);

/// Backup descriptor contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupDescriptor {
    /// Provider-owned backup name.
    pub name: String,
    /// Creation timestamp in Unix milliseconds.
    pub created_at_ms: u64,
}

/// Transaction contract for unit-of-work boundaries.
pub trait Transaction {
    /// Atomically commits the unit of work.
    fn commit(&mut self) -> StorageResult<()>;
    /// Discards the unit of work.
    fn rollback(&mut self) -> StorageResult<()>;
}

/// Repository contract for app/runtime persistence boundaries.
pub trait Repository {
    /// Returns the stable repository name.
    fn name(&self) -> &RepositoryName;
}

/// Migration contract.
pub trait Migration {
    /// Returns the stable migration identity.
    fn id(&self) -> &MigrationId;
    /// Applies this migration within an explicit transaction boundary.
    fn apply(&self, tx: &mut dyn Transaction) -> StorageResult<()>;
}

/// Storage provider contract.
pub trait StorageProvider {
    /// Returns coarse provider availability.
    fn status(&self) -> StorageStatus;
    /// Returns a current provider health snapshot.
    fn health(&self) -> StorageHealth;
    /// Opens and validates provider resources.
    fn open(&mut self) -> StorageResult<()>;
    /// Closes provider resources.
    fn close(&mut self) -> StorageResult<()>;
    /// Begins a real unit of work or fails explicitly when unsupported.
    fn begin_transaction(&mut self) -> StorageResult<Box<dyn Transaction>>;
    /// Lists provider-owned backups.
    fn list_backups(&self) -> Vec<BackupDescriptor>;
}

#[path = "storage_auth_remote.rs"]
mod storage_auth_remote;
#[path = "storage_backup.rs"]
mod storage_backup;
#[path = "storage_backup_list.rs"]
mod storage_backup_list;
#[path = "storage_file.rs"]
mod storage_file;
#[path = "storage_file_fs.rs"]
mod storage_file_fs;
#[path = "storage_tree.rs"]
mod storage_tree;
pub use storage_auth_remote::{
    data_claims, make_auth_request, now_ms, open_remote_request, open_remote_response,
    process_remote_request, seal_remote_request, seal_remote_response, transport_claims,
    validate_auth_resource, AuthRemoteRequest, AuthRemoteResponse, RemoteAuthStorageClient,
    AUTH_REMOTE_ENDPOINT, AUTH_REMOTE_SCHEMA, DEFAULT_AUTH_REMOTE_MAX_BYTES,
    DEFAULT_AUTH_REMOTE_TIMEOUT_MS, DEFAULT_AUTH_REMOTE_TTL_MS,
};
pub use storage_backup::{
    StorageBackupManifestFileV1, StorageBackupManifestV1, STORAGE_BACKUP_FORMAT_V1,
};
#[path = "storage_dnt.rs"]
mod storage_dnt;
pub use storage_dnt::{
    DntFileObjectStore, DntFileSecretStore, DntFileSnapshotStore, SealedObjectStore,
    SealedSecretStore, SealedSnapshotStore, SealedStoragePolicy,
};
pub use storage_file::FileStorageProvider;
#[cfg(test)]
pub(crate) use storage_file_fs::tmp_path_for;

#[cfg(test)]
#[path = "storage_backup_tests.rs"]
mod storage_backup_tests;
#[cfg(test)]
#[path = "storage_tests.rs"]
mod storage_tests;
