// =============================================================================
//        #######
//     ###       ###     F: application_supervisor_process.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 13:18:47 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 13:18:47 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Managed application process preparation, shutdown, and signal handling.

use super::{BootstrapError, ManagedChild, STOP_REQUESTED};
use appcore_update::UpdateError;
use std::path::{Path, PathBuf};
#[cfg(not(unix))]
use std::process::Command;
use std::process::{Child, ExitStatus};
use std::sync::atomic::Ordering;
use std::sync::Once;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// appcore-norm: allow(global-state) reason: signal handler installation must occur once per process
static CTRL_C_HANDLER_INIT: Once = Once::new();

pub(super) fn canonicalize(path: &Path, kind: &str) -> Result<PathBuf, BootstrapError> {
    std::fs::canonicalize(path).map_err(|error| {
        BootstrapError::Runtime(format!(
            "failed to resolve {kind} '{}': {error}",
            path.display()
        ))
    })
}

#[cfg(unix)]
pub(super) fn make_executable(path: &Path) -> Result<(), BootstrapError> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .map_err(|error| BootstrapError::Runtime(error.to_string()))?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions)
        .map_err(|error| BootstrapError::Runtime(error.to_string()))
}

#[cfg(not(unix))]
pub(super) fn make_executable(_path: &Path) -> Result<(), BootstrapError> {
    Ok(())
}

pub(super) fn stop_managed_child(child: &mut ManagedChild) {
    request_child_shutdown(&mut child.process);
    let _ = std::fs::remove_file(&child.health_marker);
}

#[cfg(unix)]
fn request_child_shutdown(child: &mut Child) {
    let _ = unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) };
    if wait_for_child(child, Duration::from_secs(5)) {
        return;
    }
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(not(unix))]
fn request_child_shutdown(child: &mut Child) {
    let _ = Command::new("taskkill")
        .args(["/PID", &child.id().to_string()])
        .status();
    if wait_for_child(child, Duration::from_secs(5)) {
        return;
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn wait_for_child(child: &mut Child, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(_) => return false,
        }
    }
    false
}

pub(super) fn child_error(error: std::io::Error) -> BootstrapError {
    BootstrapError::Runtime(format!("failed to inspect managed application: {error}"))
}

pub(super) fn update_error(error: UpdateError) -> BootstrapError {
    BootstrapError::Runtime(format!("managed application update failed: {error}"))
}

pub(super) fn status_text(status: ExitStatus) -> String {
    status
        .code()
        .map(|code| code.to_string())
        .unwrap_or_else(|| "signal".to_string())
}

pub(super) fn install_ctrlc_handler() -> Result<(), BootstrapError> {
    let mut result = Ok(());
    CTRL_C_HANDLER_INIT.call_once(|| {
        result = ctrlc::set_handler(|| {
            STOP_REQUESTED.store(true, Ordering::Release);
        })
        .map_err(|error| {
            BootstrapError::Runtime(format!(
                "failed to install application supervisor signal handler: {error}"
            ))
        });
    });
    result
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}
