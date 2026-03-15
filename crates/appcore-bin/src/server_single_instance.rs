// =============================================================================
//        #######
//     ###       ###     F: server_single_instance.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 20:47:13 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 00:27:19 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Operating-system lock for single-instance Runtime mode.

use crate::bootstrap::BootstrapError;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};
#[cfg(not(target_os = "linux"))]
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_os = "linux")]
const TERMINATE_TIMEOUT: Duration = Duration::from_secs(5);
const KILL_TIMEOUT: Duration = Duration::from_secs(2);
const LOCK_RETRY_INTERVAL: Duration = Duration::from_millis(100);

/// Owns the exclusive lock for one Runtime installation.
pub(super) struct PidFileGuard {
    file: File,
    path: PathBuf,
}

impl Drop for PidFileGuard {
    fn drop(&mut self) {
        let _ = FileExt::unlock(&self.file);
    }
}

impl std::fmt::Debug for PidFileGuard {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PidFileGuard")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct InstanceMetadata {
    pid: u32,
    executable: PathBuf,
    owner: String,
    start_time: String,
    application_id: String,
    instance_token: String,
}

pub(super) fn claim_single_instance(
    lock_path: &Path,
    application_id: &str,
    kill_others: bool,
) -> Result<PidFileGuard, BootstrapError> {
    validate_application_id(application_id)?;
    let mut file = open_lock_file(lock_path)?;
    match file.try_lock_exclusive() {
        Ok(()) => initialize_lock(file, lock_path, application_id),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
            resolve_lock_conflict(lock_path, application_id, kill_others)?;
            file = open_lock_file(lock_path)?;
            acquire_after_shutdown(file, lock_path, application_id)
        }
        Err(error) => Err(runtime_error(format!(
            "failed to acquire instance lock {}: {error}",
            lock_path.display()
        ))),
    }
}

fn resolve_lock_conflict(
    lock_path: &Path,
    application_id: &str,
    kill_others: bool,
) -> Result<(), BootstrapError> {
    let metadata = read_metadata(lock_path)?;
    if !kill_others {
        return Err(runtime_error(format!(
            "another AppCore instance is running for application {} with PID {}",
            metadata.application_id, metadata.pid
        )));
    }
    validate_process_identity(&metadata, application_id)?;
    terminate_validated_process(lock_path, application_id, &metadata)
}

fn terminate_validated_process(
    lock_path: &Path,
    application_id: &str,
    expected: &InstanceMetadata,
) -> Result<(), BootstrapError> {
    #[cfg(target_os = "linux")]
    {
        let current = read_metadata(lock_path)?;
        ensure_same_instance(expected, &current)?;
        validate_process_identity(&current, application_id)?;
        send_signal(current.pid, libc::SIGTERM)?;
        if wait_for_exit(current.pid, TERMINATE_TIMEOUT) {
            return Ok(());
        }
        let current = read_metadata(lock_path)?;
        ensure_same_instance(expected, &current)?;
        validate_process_identity(&current, application_id)?;
        send_signal(current.pid, libc::SIGKILL)?;
        if wait_for_exit(current.pid, KILL_TIMEOUT) {
            return Ok(());
        }
        Err(runtime_error(format!(
            "instance PID {} did not stop after SIGTERM and SIGKILL",
            current.pid
        )))
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (lock_path, application_id, expected);
        Err(runtime_error(
            "--kill-others is unsupported when complete process identity validation is unavailable",
        ))
    }
}

fn acquire_after_shutdown(
    file: File,
    lock_path: &Path,
    application_id: &str,
) -> Result<PidFileGuard, BootstrapError> {
    let deadline = Instant::now() + KILL_TIMEOUT;
    loop {
        match file.try_lock_exclusive() {
            Ok(()) => return initialize_lock(file, lock_path, application_id),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                thread::sleep(LOCK_RETRY_INTERVAL);
            }
            Err(error) => {
                return Err(runtime_error(format!(
                    "failed to acquire instance lock after shutdown: {error}"
                )));
            }
        }
    }
}

fn initialize_lock(
    mut file: File,
    lock_path: &Path,
    application_id: &str,
) -> Result<PidFileGuard, BootstrapError> {
    let metadata = current_instance_metadata(application_id)?;
    file.set_len(0)
        .and_then(|()| file.seek(SeekFrom::Start(0)).map(|_| ()))
        .and_then(|()| serde_json::to_writer(&mut file, &metadata).map_err(std::io::Error::other))
        .and_then(|()| file.write_all(b"\n"))
        .and_then(|()| file.sync_all())
        .map_err(|error| runtime_error(format!("failed to write instance metadata: {error}")))?;
    Ok(PidFileGuard {
        file,
        path: lock_path.to_path_buf(),
    })
}

fn open_lock_file(path: &Path) -> Result<File, BootstrapError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            runtime_error(format!(
                "failed to create instance lock directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    let mut options = OpenOptions::new();
    options.create(true).read(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|error| {
        runtime_error(format!(
            "failed to open instance lock {}: {error}",
            path.display()
        ))
    })
}

fn read_metadata(path: &Path) -> Result<InstanceMetadata, BootstrapError> {
    let mut input = String::new();
    File::open(path)
        .and_then(|mut file| file.read_to_string(&mut input))
        .map_err(|error| runtime_error(format!("failed to read instance metadata: {error}")))?;
    let metadata: InstanceMetadata = serde_json::from_str(&input)
        .map_err(|error| runtime_error(format!("invalid instance metadata: {error}")))?;
    validate_metadata(&metadata)?;
    Ok(metadata)
}

fn current_instance_metadata(application_id: &str) -> Result<InstanceMetadata, BootstrapError> {
    let executable = std::env::current_exe()
        .and_then(std::fs::canonicalize)
        .map_err(|error| runtime_error(format!("failed to resolve current executable: {error}")))?;
    Ok(InstanceMetadata {
        pid: std::process::id(),
        executable,
        owner: current_owner(),
        start_time: process_start_time(std::process::id())?,
        application_id: application_id.to_string(),
        instance_token: random_instance_token()?,
    })
}

fn validate_metadata(metadata: &InstanceMetadata) -> Result<(), BootstrapError> {
    if metadata.pid == 0
        || metadata.executable.as_os_str().is_empty()
        || metadata.owner.is_empty()
        || metadata.start_time.is_empty()
        || metadata.application_id.is_empty()
        || metadata.instance_token.len() != 64
    {
        return Err(runtime_error("instance metadata is incomplete"));
    }
    Ok(())
}

fn validate_application_id(application_id: &str) -> Result<(), BootstrapError> {
    if application_id.trim().is_empty() {
        return Err(runtime_error(
            "application id is required for instance lock",
        ));
    }
    Ok(())
}

fn random_instance_token() -> Result<String, BootstrapError> {
    let mut bytes = [0_u8; 32];
    getrandom::getrandom(&mut bytes)
        .map_err(|error| runtime_error(format!("failed to generate instance token: {error}")))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

#[cfg(target_os = "linux")]
fn ensure_same_instance(
    expected: &InstanceMetadata,
    current: &InstanceMetadata,
) -> Result<(), BootstrapError> {
    if expected != current {
        return Err(runtime_error(
            "instance identity changed while shutdown was in progress",
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn validate_process_identity(
    metadata: &InstanceMetadata,
    application_id: &str,
) -> Result<(), BootstrapError> {
    validate_metadata(metadata)?;
    if metadata.application_id != application_id
        || metadata.owner != process_owner(metadata.pid)?
        || metadata.start_time != process_start_time(metadata.pid)?
        || metadata.executable != process_executable(metadata.pid)?
    {
        return Err(runtime_error(
            "refusing to signal process because instance identity validation failed",
        ));
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn validate_process_identity(
    _metadata: &InstanceMetadata,
    _application_id: &str,
) -> Result<(), BootstrapError> {
    Err(runtime_error(
        "complete process identity validation is unavailable on this platform",
    ))
}

#[cfg(target_os = "linux")]
fn process_executable(pid: u32) -> Result<PathBuf, BootstrapError> {
    std::fs::read_link(format!("/proc/{pid}/exe"))
        .and_then(std::fs::canonicalize)
        .map_err(|error| runtime_error(format!("failed to resolve process executable: {error}")))
}

#[cfg(target_os = "linux")]
fn process_owner(pid: u32) -> Result<String, BootstrapError> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(format!("/proc/{pid}"))
        .map(|metadata| metadata.uid().to_string())
        .map_err(|error| runtime_error(format!("failed to resolve process owner: {error}")))
}

#[cfg(unix)]
fn current_owner() -> String {
    unsafe { libc::geteuid() }.to_string()
}

#[cfg(not(unix))]
fn current_owner() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "unknown".to_string())
}

#[cfg(target_os = "linux")]
fn process_start_time(pid: u32) -> Result<String, BootstrapError> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))
        .map_err(|error| runtime_error(format!("failed to read process start time: {error}")))?;
    let end = stat
        .rfind(')')
        .ok_or_else(|| runtime_error("invalid process stat record"))?;
    stat[end + 1..]
        .split_whitespace()
        .nth(19)
        .map(ToOwned::to_owned)
        .ok_or_else(|| runtime_error("process start time is unavailable"))
}

#[cfg(not(target_os = "linux"))]
fn process_start_time(_pid: u32) -> Result<String, BootstrapError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos().to_string())
        .map_err(|error| runtime_error(format!("system clock is before UNIX epoch: {error}")))
}

#[cfg(target_os = "linux")]
fn send_signal(pid: u32, signal: libc::c_int) -> Result<(), BootstrapError> {
    if unsafe { libc::kill(pid as libc::pid_t, signal) } == 0 {
        return Ok(());
    }
    Err(runtime_error(format!(
        "failed to send signal {signal} to PID {pid}: {}",
        std::io::Error::last_os_error()
    )))
}

#[cfg(target_os = "linux")]
fn wait_for_exit(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !Path::new(&format!("/proc/{pid}")).exists() {
            return true;
        }
        thread::sleep(LOCK_RETRY_INTERVAL);
    }
    false
}

fn runtime_error(message: impl Into<String>) -> BootstrapError {
    BootstrapError::Runtime(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exclusive_lock_rejects_a_second_claim() {
        let path = test_path("exclusive");
        let first = claim_single_instance(&path, "lock-test", false).unwrap();
        let second = claim_single_instance(&path, "lock-test", false);
        assert!(second.is_err());
        drop(first);
        assert!(claim_single_instance(&path, "lock-test", false).is_ok());
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn metadata_contains_bound_instance_identity() {
        let path = test_path("metadata");
        let guard = claim_single_instance(&path, "metadata-test", false).unwrap();
        let metadata = read_metadata(&path).unwrap();
        assert_eq!(metadata.pid, std::process::id());
        assert_eq!(metadata.application_id, "metadata-test");
        assert_eq!(metadata.instance_token.len(), 64);
        drop(guard);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn kill_request_rejects_a_different_application_identity() {
        let path = test_path("application");
        let guard = claim_single_instance(&path, "application-a", false).unwrap();
        let result = claim_single_instance(&path, "application-b", true);
        assert!(result.is_err());
        drop(guard);
        let _ = std::fs::remove_file(path);
    }

    fn test_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "appcore-instance-{name}-{}-{}.lock",
            std::process::id(),
            random_instance_token().unwrap()
        ))
    }
}
