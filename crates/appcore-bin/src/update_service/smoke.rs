// =============================================================================
//        #######
//     ###       ###     F: smoke.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded smoke-test execution for staged application artifacts.

use appcore_update::{UpdateError, UpdateResult};
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn run_smoke_test(path: &Path, timeout: Duration) -> UpdateResult<()> {
    validate_smoke_artifact(path)?;
    make_smoke_executable(path)?;
    let mut child = Command::new(path)
        .env("APPCORE_UPDATE_SMOKE_TEST", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| UpdateError::Health(error.to_string()))?;
    wait_for_smoke_test(&mut child, timeout)
}

fn wait_for_smoke_test(child: &mut std::process::Child, timeout: Duration) -> UpdateResult<()> {
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => return Ok(()),
            Ok(Some(status)) => {
                return Err(UpdateError::Health(format!(
                    "candidate smoke test exited with {status}"
                )))
            }
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            Ok(None) => return terminate_timed_out_smoke_test(child),
            Err(error) => return Err(UpdateError::Health(error.to_string())),
        }
    }
}

fn terminate_timed_out_smoke_test(child: &mut std::process::Child) -> UpdateResult<()> {
    let _ = child.kill();
    let _ = child.wait();
    Err(UpdateError::Health(
        "candidate smoke test timed out".to_string(),
    ))
}

fn validate_smoke_artifact(path: &Path) -> UpdateResult<()> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|error| UpdateError::Health(error.to_string()))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(UpdateError::Health(
            "candidate smoke artifact is not a regular file".to_string(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn make_smoke_executable(path: &Path) -> UpdateResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path)
        .map_err(|error| UpdateError::Health(error.to_string()))?
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(path, permissions)
        .map_err(|error| UpdateError::Health(error.to_string()))
}

#[cfg(not(unix))]
fn make_smoke_executable(_path: &Path) -> UpdateResult<()> {
    Ok(())
}
