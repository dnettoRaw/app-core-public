// =============================================================================
//        #######
//     ###       ###     F: state_file.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================

//! Checksummed atomic file implementation of Scheduler State Provider V1.

use crate::state::{
    validate_owner_id, DurableTaskMisfirePolicyV1, SchedulerStateClaimRequestV1,
    SchedulerStateClaimV1, SchedulerStateCompletionV1, SchedulerStateError, SchedulerStateProvider,
    SchedulerStateRecordV1, SchedulerStateRegistrationV1, SchedulerStateStatsV1,
    MAX_SCHEDULER_STATE_RECORDS,
};
use crate::state_memory::InMemorySchedulerStateProvider;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex as ProcessMutex, OnceLock, Weak};

/// Exact persistent marker for Scheduler State Provider V1 files.
pub const SCHEDULER_STATE_FORMAT_V1: &str = "appcore-scheduler-state-v1";
const MAX_STATE_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_ACTIVE_FILE_PROVIDERS: usize = 1_024;
// appcore-norm: allow(global-state) reason: atomic sequence prevents temporary path collisions
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(1);
// appcore-norm: allow(global-state) reason: bounded registry closes same-process file-lock gaps
static PROCESS_LOCKS: OnceLock<
    ProcessMutex<std::collections::HashMap<PathBuf, Weak<ProcessMutex<()>>>>,
> = OnceLock::new();

/// Durable local-filesystem scheduler state provider.
#[derive(Clone)]
pub struct FileSchedulerStateProvider {
    path: PathBuf,
    lock_path: PathBuf,
    process_lock: Arc<ProcessMutex<()>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StateFileV1 {
    format: String,
    records: Vec<FileRecordV1>,
    checksum: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRecordV1 {
    task_id: String,
    definition_hash: String,
    next_run_ms: u64,
    attempts: u32,
    misfire_policy: String,
    completed: bool,
    last_receipt_epoch: Option<u64>,
    claim: Option<FileClaimV1>,
    fencing_epoch: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileClaimV1 {
    task_id: String,
    owner_id: String,
    fencing_epoch: u64,
    lease_until_ms: u64,
    attempt: u32,
}

impl FileSchedulerStateProvider {
    /// Opens or creates a bounded V1 state file at `path`.
    pub fn new(path: impl Into<PathBuf>) -> Result<Self, SchedulerStateError> {
        let path = path.into();
        let parent = state_parent(&path);
        reject_symlink_if_present(parent)?;
        fs::create_dir_all(parent).map_err(|_| SchedulerStateError::Unavailable)?;
        reject_symlink_if_present(parent)?;
        reject_symlink_if_present(&path)?;
        let lock_path = sidecar_path(&path, ".lock");
        reject_symlink_if_present(&lock_path)?;
        let process_lock = process_lock(&path)?;
        let provider = Self {
            path,
            lock_path,
            process_lock,
        };
        provider.with_locked(false, |_| Ok(()))?;
        Ok(provider)
    }

    fn with_locked<T>(
        &self,
        write: bool,
        operation: impl FnOnce(&InMemorySchedulerStateProvider) -> Result<T, SchedulerStateError>,
    ) -> Result<T, SchedulerStateError> {
        let _process_guard = self
            .process_lock
            .lock()
            .map_err(|_| SchedulerStateError::Unavailable)?;
        reject_symlink_if_present(state_parent(&self.path))?;
        reject_symlink_if_present(&self.path)?;
        reject_symlink_if_present(&self.lock_path)?;
        let lock = open_lock(&self.lock_path)?;
        lock.lock_exclusive()
            .map_err(|_| SchedulerStateError::Unavailable)?;
        let records = load_records(&self.path)?;
        let memory = InMemorySchedulerStateProvider::from_records(records);
        let result = operation(&memory)?;
        if write {
            write_records(&self.path, memory.records())?;
        }
        Ok(result)
    }
}

fn process_lock(path: &Path) -> Result<Arc<ProcessMutex<()>>, SchedulerStateError> {
    let parent =
        fs::canonicalize(state_parent(path)).map_err(|_| SchedulerStateError::Unavailable)?;
    let file_name = path.file_name().ok_or(SchedulerStateError::Unavailable)?;
    let key = parent.join(file_name);
    let registry =
        PROCESS_LOCKS.get_or_init(|| ProcessMutex::new(std::collections::HashMap::new()));
    let mut locks = registry
        .lock()
        .map_err(|_| SchedulerStateError::Unavailable)?;
    locks.retain(|_, lock| lock.strong_count() > 0);
    if let Some(lock) = locks.get(&key).and_then(Weak::upgrade) {
        return Ok(lock);
    }
    if locks.len() >= MAX_ACTIVE_FILE_PROVIDERS {
        return Err(SchedulerStateError::Unavailable);
    }
    let lock = Arc::new(ProcessMutex::new(()));
    locks.insert(key, Arc::downgrade(&lock));
    Ok(lock)
}

impl fmt::Debug for FileSchedulerStateProvider {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("FileSchedulerStateProvider")
            .finish_non_exhaustive()
    }
}

impl SchedulerStateProvider for FileSchedulerStateProvider {
    fn register(
        &self,
        registration: &SchedulerStateRegistrationV1,
        max_records: usize,
    ) -> Result<SchedulerStateRecordV1, SchedulerStateError> {
        self.with_locked(true, |memory| memory.register(registration, max_records))
    }

    fn try_claim(
        &self,
        request: &SchedulerStateClaimRequestV1,
    ) -> Result<Option<SchedulerStateClaimV1>, SchedulerStateError> {
        self.with_locked(true, |memory| memory.try_claim(request))
    }

    fn record(&self, task_id: &str) -> Result<Option<SchedulerStateRecordV1>, SchedulerStateError> {
        self.with_locked(false, |memory| memory.record(task_id))
    }

    fn renew_claim(
        &self,
        claim: &SchedulerStateClaimV1,
        now_ms: u64,
        lease_until_ms: u64,
    ) -> Result<(), SchedulerStateError> {
        self.with_locked(true, |memory| {
            memory.renew_claim(claim, now_ms, lease_until_ms)
        })
    }

    fn complete(
        &self,
        completion: &SchedulerStateCompletionV1,
    ) -> Result<SchedulerStateRecordV1, SchedulerStateError> {
        self.with_locked(true, |memory| memory.complete(completion))
    }

    fn stats(&self) -> Result<SchedulerStateStatsV1, SchedulerStateError> {
        self.with_locked(false, |memory| memory.stats())
    }
}

fn load_records(path: &Path) -> Result<Vec<SchedulerStateRecordV1>, SchedulerStateError> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let metadata = fs::symlink_metadata(path).map_err(|_| SchedulerStateError::Unavailable)?;
    if !metadata.file_type().is_file() || metadata.len() > MAX_STATE_FILE_BYTES {
        return Err(SchedulerStateError::InvalidState("invalid state file"));
    }
    let mut file = open_regular_file(path)?;
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(MAX_STATE_FILE_BYTES.saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|_| SchedulerStateError::Unavailable)?;
    if bytes.len() as u64 > MAX_STATE_FILE_BYTES {
        return Err(SchedulerStateError::InvalidState("invalid state file"));
    }
    let file: StateFileV1 = serde_json::from_slice(&bytes)
        .map_err(|_| SchedulerStateError::InvalidState("invalid state file"))?;
    if file.format != SCHEDULER_STATE_FORMAT_V1 {
        return Err(SchedulerStateError::UpdateRequired);
    }
    if file.records.len() > MAX_SCHEDULER_STATE_RECORDS || checksum(&file.records)? != file.checksum
    {
        return Err(SchedulerStateError::InvalidState("invalid state checksum"));
    }
    let mut records = Vec::with_capacity(file.records.len());
    let mut previous = None;
    for record in file.records {
        if previous
            .as_ref()
            .is_some_and(|task_id| task_id >= &record.task_id)
        {
            return Err(SchedulerStateError::InvalidState("invalid state ordering"));
        }
        previous = Some(record.task_id.clone());
        records.push(record.try_into()?);
    }
    Ok(records)
}

fn write_records(
    path: &Path,
    records: Vec<SchedulerStateRecordV1>,
) -> Result<(), SchedulerStateError> {
    let file_records = records
        .into_iter()
        .map(FileRecordV1::from)
        .collect::<Vec<_>>();
    let file = StateFileV1 {
        format: SCHEDULER_STATE_FORMAT_V1.to_string(),
        checksum: checksum(&file_records)?,
        records: file_records,
    };
    let bytes = serde_json::to_vec(&file).map_err(|_| SchedulerStateError::Unavailable)?;
    if bytes.len() as u64 > MAX_STATE_FILE_BYTES {
        return Err(SchedulerStateError::CapacityExceeded {
            max_records: MAX_SCHEDULER_STATE_RECORDS,
        });
    }
    atomic_write(path, &bytes)
}

impl From<SchedulerStateRecordV1> for FileRecordV1 {
    fn from(record: SchedulerStateRecordV1) -> Self {
        Self {
            task_id: record.task_id,
            definition_hash: record.definition_hash,
            next_run_ms: record.next_run_ms,
            attempts: record.attempts,
            misfire_policy: match record.misfire_policy {
                DurableTaskMisfirePolicyV1::FireOnce => "fire_once",
                DurableTaskMisfirePolicyV1::Skip => "skip",
            }
            .to_string(),
            completed: record.completed,
            last_receipt_epoch: record.last_receipt_epoch,
            claim: record.claim.map(FileClaimV1::from),
            fencing_epoch: record.fencing_epoch,
        }
    }
}

impl TryFrom<FileRecordV1> for SchedulerStateRecordV1 {
    type Error = SchedulerStateError;

    fn try_from(record: FileRecordV1) -> Result<Self, Self::Error> {
        let misfire_policy = match record.misfire_policy.as_str() {
            "fire_once" => DurableTaskMisfirePolicyV1::FireOnce,
            "skip" => DurableTaskMisfirePolicyV1::Skip,
            _ => return Err(SchedulerStateError::UpdateRequired),
        };
        let claim = record
            .claim
            .map(SchedulerStateClaimV1::try_from)
            .transpose()?;
        let record = Self {
            task_id: record.task_id,
            definition_hash: record.definition_hash,
            next_run_ms: record.next_run_ms,
            attempts: record.attempts,
            misfire_policy,
            completed: record.completed,
            last_receipt_epoch: record.last_receipt_epoch,
            claim,
            fencing_epoch: record.fencing_epoch,
        };
        record.validate()?;
        Ok(record)
    }
}

impl From<SchedulerStateClaimV1> for FileClaimV1 {
    fn from(claim: SchedulerStateClaimV1) -> Self {
        Self {
            task_id: claim.task_id,
            owner_id: claim.owner_id,
            fencing_epoch: claim.fencing_epoch,
            lease_until_ms: claim.lease_until_ms,
            attempt: claim.attempt,
        }
    }
}

impl TryFrom<FileClaimV1> for SchedulerStateClaimV1 {
    type Error = SchedulerStateError;

    fn try_from(claim: FileClaimV1) -> Result<Self, Self::Error> {
        validate_owner_id(&claim.owner_id)?;
        if claim.fencing_epoch == 0 || claim.attempt == 0 {
            return Err(SchedulerStateError::InvalidState("invalid claim state"));
        }
        Self::new(
            claim.task_id,
            claim.owner_id,
            claim.fencing_epoch,
            claim.lease_until_ms,
            claim.attempt,
        )
    }
}

fn checksum(records: &[FileRecordV1]) -> Result<String, SchedulerStateError> {
    let bytes = serde_json::to_vec(records).map_err(|_| SchedulerStateError::Unavailable)?;
    let digest = Sha256::digest(bytes);
    Ok(digest.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), SchedulerStateError> {
    let temporary = sidecar_path(
        path,
        &format!(
            ".tmp-{}-{}",
            std::process::id(),
            TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ),
    );
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    let mut file =
        open_no_follow(&mut options, &temporary).map_err(|_| SchedulerStateError::Unavailable)?;
    let result = (|| {
        file.write_all(bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| SchedulerStateError::Unavailable)?;
        fs::rename(&temporary, path).map_err(|_| SchedulerStateError::Unavailable)?;
        sync_parent(path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn open_lock(path: &Path) -> Result<File, SchedulerStateError> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    let file = open_no_follow(&mut options, path).map_err(|_| SchedulerStateError::Unavailable)?;
    validate_open_file(&file, path)?;
    Ok(file)
}

fn sync_parent(path: &Path) -> Result<(), SchedulerStateError> {
    File::open(state_parent(path))
        .and_then(|directory| directory.sync_all())
        .map_err(|_| SchedulerStateError::Unavailable)
}

fn state_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut value: OsString = path.as_os_str().to_owned();
    value.push(suffix);
    PathBuf::from(value)
}

fn reject_symlink_if_present(path: &Path) -> Result<(), SchedulerStateError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata_is_link(&metadata) => Err(SchedulerStateError::Unavailable),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(SchedulerStateError::Unavailable),
    }
}

fn open_regular_file(path: &Path) -> Result<File, SchedulerStateError> {
    let mut options = OpenOptions::new();
    options.read(true);
    let file = open_no_follow(&mut options, path).map_err(|_| SchedulerStateError::Unavailable)?;
    validate_open_file(&file, path)?;
    Ok(file)
}

fn validate_open_file(file: &File, path: &Path) -> Result<(), SchedulerStateError> {
    let path_metadata = fs::symlink_metadata(path).map_err(|_| SchedulerStateError::Unavailable)?;
    let file_metadata = file
        .metadata()
        .map_err(|_| SchedulerStateError::Unavailable)?;
    if metadata_is_link(&path_metadata)
        || metadata_is_link(&file_metadata)
        || !file_metadata.is_file()
    {
        return Err(SchedulerStateError::Unavailable);
    }
    Ok(())
}

#[cfg(unix)]
fn open_no_follow(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    options.custom_flags(libc::O_NOFOLLOW).open(path)
}

#[cfg(windows)]
fn open_no_follow(options: &mut OpenOptions, path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT;
    options
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(all(not(unix), not(windows)))]
fn open_no_follow(_options: &mut OpenOptions, _path: &Path) -> io::Result<File> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "no-follow file opening is unavailable on this platform",
    ))
}

fn metadata_is_link(metadata: &fs::Metadata) -> bool {
    metadata.file_type().is_symlink() || metadata_is_reparse_point(metadata)
}

#[cfg(windows)]
fn metadata_is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    use windows_sys::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn metadata_is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}
