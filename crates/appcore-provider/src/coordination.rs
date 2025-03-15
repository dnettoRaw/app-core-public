// =============================================================================
//        #######
//     ###       ###     F: coordination.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 10:59:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 14:12:17 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::{ProviderError, ProviderResult};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

/// Latest coordination schema understood by this Runtime release.
pub const COORDINATION_SCHEMA_VERSION: u64 = 2;

/// Runtime-owned coordination tables allowed by schema version 2.
pub const COORDINATION_TABLES: &[&str] = &[
    "audit",
    "capabilities",
    "jobs",
    "leases",
    "runtime_instances",
    "runtime_versions",
    "schema_migrations",
    "tenants",
];

const STORE_FORMAT: &str = "appcore.coordination-store.v1";
const METADATA_FILE: &str = "coordination-schema.meta";
// appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
static TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// Backend contract for the Runtime-owned coordination schema.
///
/// Runtime nodes normally reach this store through a control-plane provider.
/// This contract exists for control-plane implementations and deliberately does
/// not expose generic business-data reads or writes.
pub trait CoordinationStoreProvider: Send + Sync {
    /// Returns the latest migration version applied by the provider.
    fn schema_version(&self) -> ProviderResult<u64>;

    /// Verifies backend connectivity and schema access.
    fn health(&self) -> ProviderResult<()>;

    /// Rejects stores that do not implement the schema required by this Runtime.
    fn ensure_compatible(&self) -> ProviderResult<()> {
        self.health()?;
        let actual = self.schema_version()?;
        if actual < COORDINATION_SCHEMA_VERSION {
            return Err(ProviderError::InvalidConfiguration(format!(
                "coordination schema {actual} is older than required schema {COORDINATION_SCHEMA_VERSION}"
            )));
        }
        Ok(())
    }
}

/// Deterministic coordination store for embedded control planes and tests.
#[derive(Debug)]
pub struct InMemoryCoordinationStore {
    schema_version: u64,
    healthy: AtomicBool,
}

impl Default for InMemoryCoordinationStore {
    fn default() -> Self {
        Self {
            schema_version: COORDINATION_SCHEMA_VERSION,
            healthy: AtomicBool::new(true),
        }
    }
}

impl InMemoryCoordinationStore {
    /// Creates a store at an explicit schema version for migration tests.
    pub fn with_schema_version(schema_version: u64) -> Self {
        Self {
            schema_version,
            healthy: AtomicBool::new(true),
        }
    }

    /// Changes the deterministic health result.
    pub fn set_healthy(&self, healthy: bool) {
        self.healthy.store(healthy, Ordering::SeqCst);
    }
}

impl CoordinationStoreProvider for InMemoryCoordinationStore {
    fn schema_version(&self) -> ProviderResult<u64> {
        Ok(self.schema_version)
    }

    fn health(&self) -> ProviderResult<()> {
        if self.healthy.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(ProviderError::Initialization(
                "in-memory coordination store is unhealthy".to_string(),
            ))
        }
    }
}

/// Durable single-control-plane coordination schema store.
///
/// The file provider certifies schema ownership and crash-consistent metadata
/// for deployments in which one control-plane service owns the data directory.
/// Runtime nodes never receive database credentials through this contract.
#[derive(Debug)]
pub struct FileCoordinationStore {
    root: PathBuf,
    metadata_path: PathBuf,
    lock: Mutex<()>,
}

impl FileCoordinationStore {
    /// Opens a store, creating or transactionally migrating metadata to V2.
    pub fn open(root: impl Into<PathBuf>) -> ProviderResult<Self> {
        let root = root.into();
        prepare_root(&root)?;
        let store = Self {
            metadata_path: root.join(METADATA_FILE),
            root,
            lock: Mutex::new(()),
        };
        store.migrate()?;
        store.ensure_compatible()?;
        Ok(store)
    }

    /// Returns the deployment-owned data directory.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Writes a validated metadata backup using atomic replacement.
    pub fn backup_to(&self, destination: impl AsRef<Path>) -> ProviderResult<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| initialization("coordination store lock poisoned"))?;
        let metadata = self.read_metadata()?;
        validate_metadata(&metadata)?;
        write_atomic(destination.as_ref(), render_metadata(&metadata).as_bytes())
    }

    /// Restores validated metadata and reruns forward-only migrations.
    pub fn restore_from(&self, source: impl AsRef<Path>) -> ProviderResult<()> {
        let contents = fs::read_to_string(source.as_ref())
            .map_err(|error| initialization(format!("coordination backup read failed: {error}")))?;
        let metadata = parse_metadata(&contents)?;
        validate_metadata(&metadata)?;
        let _guard = self
            .lock
            .lock()
            .map_err(|_| initialization("coordination store lock poisoned"))?;
        write_atomic(&self.metadata_path, render_metadata(&metadata).as_bytes())?;
        drop(_guard);
        self.migrate()
    }

    fn migrate(&self) -> ProviderResult<()> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| initialization("coordination store lock poisoned"))?;
        let metadata = if self.metadata_path.exists() {
            let current = self.read_metadata()?;
            migrate_metadata(current)?
        } else {
            CoordinationMetadata::latest()
        };
        write_atomic(&self.metadata_path, render_metadata(&metadata).as_bytes())
    }

    fn read_metadata(&self) -> ProviderResult<CoordinationMetadata> {
        reject_symlink(&self.metadata_path)?;
        let contents = fs::read_to_string(&self.metadata_path).map_err(|error| {
            initialization(format!("coordination metadata read failed: {error}"))
        })?;
        parse_metadata(&contents)
    }
}

impl CoordinationStoreProvider for FileCoordinationStore {
    fn schema_version(&self) -> ProviderResult<u64> {
        let _guard = self
            .lock
            .lock()
            .map_err(|_| initialization("coordination store lock poisoned"))?;
        Ok(self.read_metadata()?.version)
    }

    fn health(&self) -> ProviderResult<()> {
        reject_symlink(&self.root)?;
        let metadata = self.read_metadata()?;
        validate_metadata(&metadata)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CoordinationMetadata {
    version: u64,
    tables: Vec<String>,
}

impl CoordinationMetadata {
    fn latest() -> Self {
        Self {
            version: COORDINATION_SCHEMA_VERSION,
            tables: COORDINATION_TABLES
                .iter()
                .map(|table| (*table).to_string())
                .collect(),
        }
    }
}

fn migrate_metadata(mut metadata: CoordinationMetadata) -> ProviderResult<CoordinationMetadata> {
    if metadata.version > COORDINATION_SCHEMA_VERSION {
        return Err(ProviderError::InvalidConfiguration(format!(
            "coordination schema {} is newer than supported schema {}",
            metadata.version, COORDINATION_SCHEMA_VERSION
        )));
    }
    if metadata.version == 0 {
        return Err(ProviderError::InvalidConfiguration(
            "coordination schema version must be positive".to_string(),
        ));
    }
    metadata.version = COORDINATION_SCHEMA_VERSION;
    metadata.tables = COORDINATION_TABLES
        .iter()
        .map(|table| (*table).to_string())
        .collect();
    Ok(metadata)
}

fn parse_metadata(contents: &str) -> ProviderResult<CoordinationMetadata> {
    let mut format = None;
    let mut version = None;
    let mut tables = None;
    for line in contents.lines().filter(|line| !line.trim().is_empty()) {
        let (key, value) = line.split_once('=').ok_or_else(|| {
            ProviderError::InvalidConfiguration("invalid coordination metadata".to_string())
        })?;
        match key {
            "format" => format = Some(value),
            "version" => {
                version = Some(value.parse::<u64>().map_err(|_| {
                    ProviderError::InvalidConfiguration(
                        "invalid coordination schema version".to_string(),
                    )
                })?)
            }
            "tables" => {
                tables = Some(value.split(',').map(str::to_string).collect::<Vec<_>>());
            }
            _ => {
                return Err(ProviderError::InvalidConfiguration(format!(
                    "unknown coordination metadata field: {key}"
                )))
            }
        }
    }
    if format != Some(STORE_FORMAT) {
        return Err(ProviderError::InvalidConfiguration(
            "unsupported coordination metadata format".to_string(),
        ));
    }
    Ok(CoordinationMetadata {
        version: version.ok_or_else(|| {
            ProviderError::InvalidConfiguration("missing coordination schema version".to_string())
        })?,
        tables: tables.ok_or_else(|| {
            ProviderError::InvalidConfiguration("missing coordination table allowlist".to_string())
        })?,
    })
}

fn render_metadata(metadata: &CoordinationMetadata) -> String {
    format!(
        "format={STORE_FORMAT}\nversion={}\ntables={}\n",
        metadata.version,
        metadata.tables.join(",")
    )
}

fn validate_metadata(metadata: &CoordinationMetadata) -> ProviderResult<()> {
    if metadata.version > COORDINATION_SCHEMA_VERSION {
        return Err(ProviderError::InvalidConfiguration(
            "coordination schema is newer than this Runtime".to_string(),
        ));
    }
    let expected = COORDINATION_TABLES
        .iter()
        .map(|table| (*table).to_string())
        .collect::<Vec<_>>();
    if metadata.tables != expected {
        return Err(ProviderError::InvalidConfiguration(
            "coordination table allowlist does not match schema V2".to_string(),
        ));
    }
    Ok(())
}

fn prepare_root(root: &Path) -> ProviderResult<()> {
    reject_symlink(root)?;
    fs::create_dir_all(root)
        .map_err(|error| initialization(format!("coordination directory failed: {error}")))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(root, fs::Permissions::from_mode(0o700)).map_err(|error| {
            initialization(format!(
                "coordination directory permissions failed: {error}"
            ))
        })?;
    }
    Ok(())
}

fn reject_symlink(path: &Path) -> ProviderResult<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            Err(ProviderError::InvalidConfiguration(
                "coordination path cannot be a symlink".to_string(),
            ))
        }
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(initialization(format!(
            "coordination path inspection failed: {error}"
        ))),
    }
}

fn write_atomic(path: &Path, bytes: &[u8]) -> ProviderResult<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|error| initialization(format!("coordination parent failed: {error}")))?;
    let temp = parent.join(format!(
        ".coordination.{}.{}.tmp",
        std::process::id(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let result = write_and_replace(&temp, path, parent, bytes);
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

fn write_and_replace(temp: &Path, path: &Path, _parent: &Path, bytes: &[u8]) -> ProviderResult<()> {
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(temp)
        .map_err(|error| initialization(format!("coordination temp create failed: {error}")))?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|error| initialization(format!("coordination metadata write failed: {error}")))?;
    fs::rename(temp, path).map_err(|error| {
        initialization(format!("coordination metadata replace failed: {error}"))
    })?;
    #[cfg(unix)]
    fs::File::open(_parent)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| initialization(format!("coordination directory sync failed: {error}")))?;
    Ok(())
}

fn initialization(message: impl Into<String>) -> ProviderError {
    ProviderError::Initialization(message.into())
}
