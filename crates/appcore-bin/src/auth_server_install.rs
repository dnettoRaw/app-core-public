// =============================================================================
//        #######
//     ###       ###     F: auth_server_install.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/06 22:13:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/07 09:02:12 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Installs the optional auth-server companion binary.

use crate::bootstrap::BootstrapError;
use crate::constants::{auth_server_app_gate_matches, AUTH_SERVER_BIN};
use crate::paths::AppCorePaths;
use appcore_security::{format_secret_material, new_rotated_secret};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn install_auth_server_if_requested(
    paths: &AppCorePaths,
    password: Option<&str>,
) -> Result<(), BootstrapError> {
    let Some(password) = password else {
        return Ok(());
    };
    let source = auth_server_source_path()?;
    install_auth_server_from_source(paths, password, &source)
}

fn install_auth_server_from_source(
    paths: &AppCorePaths,
    password: &str,
    source: &Path,
) -> Result<(), BootstrapError> {
    validate_auth_server_gate(password)?;
    ensure_auth_server_source(source)?;
    prepare_auth_server_dirs(paths)?;
    write_auth_server_secret_if_missing(paths)?;
    write_auth_transport_secret_if_missing(paths)?;
    write_auth_required_marker(paths)?;
    copy_auth_server_binary(paths, source)?;
    print_auth_server_install_plan(paths);
    Ok(())
}

fn prepare_auth_server_dirs(paths: &AppCorePaths) -> Result<(), BootstrapError> {
    fs::create_dir_all(paths.bins_dir()).map_err(|err| BootstrapError::Runtime(err.to_string()))?;
    fs::create_dir_all(paths.security_dir()).map_err(|err| BootstrapError::Runtime(err.to_string()))
}

fn write_auth_server_secret_if_missing(paths: &AppCorePaths) -> Result<(), BootstrapError> {
    write_secret_if_missing(
        paths.auth_server_secret_file(),
        "auth-server data secret generation failed",
    )
}

fn write_auth_transport_secret_if_missing(paths: &AppCorePaths) -> Result<(), BootstrapError> {
    write_secret_if_missing(
        paths.auth_transport_secret_file(),
        "auth transport secret generation failed",
    )
}

fn write_auth_required_marker(paths: &AppCorePaths) -> Result<(), BootstrapError> {
    let body = format!(
        "{{\"schema\":\"appcore.auth-required.v1\",\"app_id\":\"{}\",\"data_secret\":\"auth-server.secret\",\"transport_secret\":\"auth-transport.secret\"}}\n",
        paths.app_id
    );
    fs::write(paths.auth_required_file(), body)
        .map_err(|err| BootstrapError::Runtime(err.to_string()))
}

fn write_secret_if_missing(path: PathBuf, error: &str) -> Result<(), BootstrapError> {
    if path.exists() {
        return Ok(());
    }
    let material =
        new_rotated_secret(None).map_err(|_| BootstrapError::Runtime(error.to_string()))?;
    fs::write(path, format_secret_material(&material))
        .map_err(|err| BootstrapError::Runtime(err.to_string()))
}

fn copy_auth_server_binary(paths: &AppCorePaths, source: &Path) -> Result<(), BootstrapError> {
    fs::copy(source, paths.auth_server_file())
        .map_err(|err| BootstrapError::Runtime(err.to_string()))?;
    Ok(())
}

fn print_auth_server_install_plan(paths: &AppCorePaths) {
    println!(
        "auth_server_installed: {}",
        paths.auth_server_file().display()
    );
    println!(
        "auth_server_secret_file: {}",
        paths.auth_server_secret_file().display()
    );
    println!(
        "auth_transport_secret_file: {}",
        paths.auth_transport_secret_file().display()
    );
    println!(
        "auth_required_file: {}",
        paths.auth_required_file().display()
    );
}

fn validate_auth_server_gate(password: &str) -> Result<(), BootstrapError> {
    if auth_server_app_gate_matches(password) {
        return Ok(());
    }
    Err(BootstrapError::Cli(
        "invalid --auth-server-app password".to_string(),
    ))
}

fn ensure_auth_server_source(source: &Path) -> Result<(), BootstrapError> {
    if source.exists() {
        return Ok(());
    }
    Err(BootstrapError::Runtime(format!(
        "auth server binary not found: {}",
        source.display()
    )))
}

fn auth_server_source_path() -> Result<PathBuf, BootstrapError> {
    if let Some(path) = std::env::var_os("APPCORE_AUTH_SERVER_SOURCE") {
        return Ok(PathBuf::from(path));
    }
    let current =
        std::env::current_exe().map_err(|err| BootstrapError::Runtime(err.to_string()))?;
    let dir = current.parent().ok_or_else(|| {
        BootstrapError::Runtime("current executable has no parent directory".to_string())
    })?;
    Ok(dir.join(auth_server_source_file_name()))
}

fn auth_server_source_file_name() -> String {
    #[cfg(windows)]
    {
        format!("{AUTH_SERVER_BIN}.exe")
    }
    #[cfg(not(windows))]
    {
        AUTH_SERVER_BIN.to_string()
    }
}

#[cfg(test)]
#[path = "auth_server_install_tests.rs"]
mod tests;
