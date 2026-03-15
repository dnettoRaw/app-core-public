// =============================================================================
//        #######
//     ###       ###     F: local_lifecycle.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/05 22:12:31 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Local first-run and cleanup lifecycle for appcore-bin.

use crate::auth_server_install::install_auth_server_if_requested;
use crate::bootstrap::{now_ms, BootstrapError};
use crate::paths::{print_paths, AppCorePaths};
use appcore_contracts::{
    ApplicationId, ApplicationManifestV1, DeploymentManifestV1, InstallationId, NetworkConfig,
    ProviderConfig, ProviderId, RuntimeMode, RuntimeRequirements, SecretRef, ServiceId,
};
use appcore_security::{format_secret_material, new_rotated_secret};
use std::fs;
use std::path::Path;

pub fn run_first_run(auth_password: Option<&str>) -> Result<(), BootstrapError> {
    let paths = AppCorePaths::from_env().map_err(BootstrapError::Cli)?;
    first_run_with_auth_server(&paths, auth_password)?;
    print_paths(&paths);
    Ok(())
}

pub fn run_last_run(dry_run: bool, purge: bool) -> Result<(), BootstrapError> {
    let paths = AppCorePaths::from_env().map_err(BootstrapError::Cli)?;
    last_run(&paths, dry_run, purge)?;
    Ok(())
}

pub fn run_paths() -> Result<(), BootstrapError> {
    let paths = AppCorePaths::from_env().map_err(BootstrapError::Cli)?;
    print_paths(&paths);
    Ok(())
}

pub fn first_run(paths: &AppCorePaths) -> Result<(), BootstrapError> {
    first_run_with_auth_server(paths, None)
}

pub fn first_run_with_auth_server(
    paths: &AppCorePaths,
    auth_password: Option<&str>,
) -> Result<(), BootstrapError> {
    create_local_dirs(paths)?;
    write_secret_if_missing(&paths.secret_file)?;
    write_manifests_if_missing(paths)?;
    write_dnt_files(paths)?;
    install_auth_server_if_requested(paths, auth_password)?;
    Ok(())
}

pub fn last_run(paths: &AppCorePaths, dry_run: bool, purge: bool) -> Result<(), BootstrapError> {
    if !paths.safe_for_last_run() {
        return Err(BootstrapError::Cli(
            "refusing unsafe local cleanup path".to_string(),
        ));
    }
    print_last_run_plan(paths, purge);
    if dry_run {
        return Ok(());
    }
    remove_dir_if_exists(&paths.data_dir)?;
    if purge {
        remove_dir_if_exists(&paths.cache_dir)?;
    }
    Ok(())
}

fn create_local_dirs(paths: &AppCorePaths) -> Result<(), BootstrapError> {
    for dir in local_dirs(paths) {
        fs::create_dir_all(dir).map_err(|err| BootstrapError::Runtime(err.to_string()))?;
    }
    Ok(())
}

fn local_dirs(paths: &AppCorePaths) -> [&Path; 7] {
    [
        &paths.config_dir,
        &paths.data_dir,
        &paths.logs_dir,
        &paths.cache_dir,
        &paths.runtime_dir,
        &paths.backups_dir,
        &paths.identity_dir,
    ]
}

fn write_secret_if_missing(path: &Path) -> Result<(), BootstrapError> {
    if path.exists() {
        return Ok(());
    }
    let material = new_rotated_secret(None)
        .map_err(|_| BootstrapError::Runtime("security secret generation failed".to_string()))?;
    fs::write(path, format_secret_material(&material))
        .map_err(|err| BootstrapError::Runtime(err.to_string()))
}

fn write_manifests_if_missing(paths: &AppCorePaths) -> Result<(), BootstrapError> {
    if !paths.application_manifest.exists() {
        fs::write(
            &paths.application_manifest,
            local_application_manifest(paths)?,
        )
        .map_err(|error| BootstrapError::Runtime(error.to_string()))?;
    }
    if !paths.deployment_manifest.exists() {
        fs::write(
            &paths.deployment_manifest,
            local_deployment_manifest(paths)?,
        )
        .map_err(|error| BootstrapError::Runtime(error.to_string()))?;
    }
    Ok(())
}

fn write_dnt_files(paths: &AppCorePaths) -> Result<(), BootstrapError> {
    write_dnt(
        &paths.identity_dir.join("identity.dnt"),
        "appcore.identity.v1",
        &paths.app_id,
    )?;
    write_dnt(
        &paths.runtime_dir.join("runtime.dnt"),
        "appcore.runtime.v1",
        &paths.app_id,
    )?;
    write_dnt(
        &paths.config_dir.join("install.dnt"),
        "appcore.install.v1",
        &paths.app_id,
    )
}

fn write_dnt(path: &Path, schema: &str, app_id: &str) -> Result<(), BootstrapError> {
    if path.exists() {
        return Ok(());
    }
    let body = format!(
        "{{\"schema\":\"{schema}\",\"app_id\":\"{app_id}\",\"created_at_ms\":{}}}\n",
        now_ms()
    );
    fs::write(path, body).map_err(|err| BootstrapError::Runtime(err.to_string()))
}

fn local_application_manifest(paths: &AppCorePaths) -> Result<String, BootstrapError> {
    let application_id = ApplicationId::new(paths.app_id.clone()).map_err(contract_error)?;
    let requirements =
        RuntimeRequirements::new(env!("CARGO_PKG_VERSION"), "1").map_err(contract_error)?;
    let manifest = ApplicationManifestV1::new(
        application_id,
        env!("CARGO_PKG_VERSION"),
        paths.app_name.clone(),
        "local",
        ServiceId::new("app.host").map_err(contract_error)?,
        requirements,
    )
    .map_err(contract_error)?;
    toml::to_string_pretty(&manifest).map_err(|error| BootstrapError::Runtime(error.to_string()))
}

fn local_deployment_manifest(paths: &AppCorePaths) -> Result<String, BootstrapError> {
    let network = NetworkConfig::new(
        ProviderId::new("http").map_err(contract_error)?,
        ProviderId::new("http").map_err(contract_error)?,
    )
    .with_listen_address("127.0.0.1:39001")
    .map_err(contract_error)?;
    let manifest = DeploymentManifestV1::builder(
        InstallationId::new(format!("{}-local", paths.app_id)).map_err(contract_error)?,
        ApplicationId::new(paths.app_id.clone()).map_err(contract_error)?,
        RuntimeMode::Standalone,
        ProviderConfig::new(ProviderId::new("file").map_err(contract_error)?),
        network,
    )
    .with_path("storage", paths.runtime_dir.display().to_string())
    .and_then(|builder| builder.with_path("backup", paths.backups_dir.display().to_string()))
    .and_then(|builder| {
        builder.with_secret(
            "runtime_security",
            SecretRef::new(format!("file:{}", paths.secret_file.display()))?,
        )
    })
    .and_then(|builder| builder.build())
    .map_err(contract_error)?;
    toml::to_string_pretty(&manifest).map_err(|error| BootstrapError::Runtime(error.to_string()))
}

fn contract_error(error: impl std::fmt::Display) -> BootstrapError {
    BootstrapError::Runtime(error.to_string())
}

fn print_last_run_plan(paths: &AppCorePaths, purge: bool) {
    println!("remove data_dir: {}", paths.data_dir.display());
    if purge {
        println!("remove cache_dir: {}", paths.cache_dir.display());
    } else {
        println!("keep cache_dir: {}", paths.cache_dir.display());
    }
}

fn remove_dir_if_exists(path: &Path) -> Result<(), BootstrapError> {
    if !path.exists() {
        return Ok(());
    }
    fs::remove_dir_all(path).map_err(|err| BootstrapError::Runtime(err.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::PathInputs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    // appcore-norm: allow(global-state) reason: atomic sequence prevents process-local temporary path collisions
    static TEMP_PATH_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_paths(name: &str) -> AppCorePaths {
        let root = std::env::temp_dir().join(format!(
            "appcore-bin-{name}-{}-{}-{}",
            std::process::id(),
            now_ms(),
            TEMP_PATH_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        AppCorePaths::from_inputs(PathInputs {
            app_name: "DemoHost".to_string(),
            app_id: "demo".to_string(),
            home: Some(root.join("home")),
            data_root: Some(root.join("AppCore-Runtime")),
            cache_root: Some(root.join("AppCore-Runtime-cache")),
        })
        .expect("paths")
    }

    #[test]
    fn first_run_creates_dirs_and_dnt_files() {
        let paths = temp_paths("first-run");
        first_run(&paths).expect("first run");
        assert!(paths.config_dir.exists());
        assert!(paths.cache_dir.exists());
        assert!(paths.identity_dir.join("identity.dnt").exists());
        assert!(paths.runtime_dir.join("runtime.dnt").exists());
        assert!(paths.config_dir.join("install.dnt").exists());
    }

    #[test]
    fn first_run_is_idempotent() {
        let paths = temp_paths("idempotent");
        first_run(&paths).expect("first run");
        let application =
            fs::read_to_string(&paths.application_manifest).expect("application manifest");
        let deployment =
            fs::read_to_string(&paths.deployment_manifest).expect("deployment manifest");
        first_run(&paths).expect("second first run");
        assert_eq!(
            application,
            fs::read_to_string(&paths.application_manifest).expect("application manifest")
        );
        assert_eq!(
            deployment,
            fs::read_to_string(&paths.deployment_manifest).expect("deployment manifest")
        );
    }

    #[test]
    fn last_run_dry_run_keeps_dirs() {
        let paths = temp_paths("dry-run");
        first_run(&paths).expect("first run");
        last_run(&paths, true, true).expect("dry run");
        assert!(paths.data_dir.exists());
        assert!(paths.cache_dir.exists());
    }

    #[test]
    fn last_run_purge_removes_cache() {
        let paths = temp_paths("purge");
        first_run(&paths).expect("first run");
        last_run(&paths, false, true).expect("purge");
        assert!(!paths.data_dir.exists());
        assert!(!paths.cache_dir.exists());
    }

    #[test]
    fn last_run_without_purge_keeps_cache() {
        let paths = temp_paths("keep-cache");
        first_run(&paths).expect("first run");
        last_run(&paths, false, false).expect("cleanup");
        assert!(!paths.data_dir.exists());
        assert!(paths.cache_dir.exists());
    }

    #[test]
    fn last_run_rejects_unsafe_paths() {
        let paths = AppCorePaths::from_inputs(PathInputs {
            app_name: "DemoHost".to_string(),
            app_id: "demo".to_string(),
            home: Some(PathBuf::from("/tmp/home")),
            data_root: Some(PathBuf::from("/tmp/not-runtime")),
            cache_root: Some(PathBuf::from("/tmp/not-runtime-cache")),
        })
        .expect("paths");
        assert!(last_run(&paths, true, false).is_err());
    }
}
