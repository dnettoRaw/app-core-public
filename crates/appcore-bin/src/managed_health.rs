// =============================================================================
//        #######
//     ###       ###     F: managed_health.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Process-local readiness marker shared with the managed application supervisor.

use crate::bootstrap::BootstrapError;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

pub(crate) const MANAGED_HEALTH_FILE_ENV: &str = "APPCORE_MANAGED_HEALTH_FILE";

pub(crate) struct ManagedHealthGuard {
    path: Option<PathBuf>,
}

impl ManagedHealthGuard {
    pub(crate) fn ready() -> Result<Self, BootstrapError> {
        let Some(path) = std::env::var_os(MANAGED_HEALTH_FILE_ENV).map(PathBuf::from) else {
            return Ok(Self { path: None });
        };
        let parent = path.parent().ok_or_else(|| {
            BootstrapError::Runtime("managed health marker has no parent directory".to_string())
        })?;
        fs::create_dir_all(parent).map_err(marker_error)?;
        let temporary = path.with_extension(format!("tmp-{}", std::process::id()));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(marker_error)?;
        writeln!(file, "ready").map_err(marker_error)?;
        file.sync_all().map_err(marker_error)?;
        fs::rename(&temporary, &path).map_err(marker_error)?;
        if let Ok(directory) = File::open(parent) {
            let _ = directory.sync_all();
        }
        Ok(Self { path: Some(path) })
    }
}

impl Drop for ManagedHealthGuard {
    fn drop(&mut self) {
        if let Some(path) = &self.path {
            let _ = fs::remove_file(path);
        }
    }
}

fn marker_error(error: std::io::Error) -> BootstrapError {
    BootstrapError::Runtime(format!("managed health marker failed: {error}"))
}
