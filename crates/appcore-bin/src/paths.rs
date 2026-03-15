// =============================================================================
//        #######
//     ###       ###     F: paths.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/05 22:12:31 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/02 14:12:17 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Platform path policy for local AppCore-Runtime installs.

use crate::constants::{
    auth_server_install_name, env_or_default, DEFAULT_APP_ID, DEFAULT_APP_NAME,
};
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppCorePaths {
    pub app_name: String,
    pub app_id: String,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub config_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub runtime_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub identity_dir: PathBuf,
    pub application_manifest: PathBuf,
    pub deployment_manifest: PathBuf,
    pub secret_file: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathInputs {
    pub app_name: String,
    pub app_id: String,
    pub home: Option<PathBuf>,
    pub data_root: Option<PathBuf>,
    pub cache_root: Option<PathBuf>,
}

impl Default for PathInputs {
    fn default() -> Self {
        Self {
            app_name: env_or_default("APPCORE_APP_NAME", DEFAULT_APP_NAME),
            app_id: env::var("APPCORE_APP_ID").unwrap_or_else(|_| DEFAULT_APP_ID.to_string()),
            home: env::var_os("HOME").map(PathBuf::from),
            data_root: env::var_os("APPCORE_DATA_DIR").map(PathBuf::from),
            cache_root: env::var_os("APPCORE_CACHE_DIR").map(PathBuf::from),
        }
    }
}

impl AppCorePaths {
    pub fn from_env() -> Result<Self, String> {
        Self::from_inputs(PathInputs::default())
    }

    pub fn from_inputs(inputs: PathInputs) -> Result<Self, String> {
        let data_dir = data_root(&inputs)?.join(&inputs.app_id);
        let cache_dir = cache_root(&inputs)?.join(&inputs.app_id);
        Ok(Self::from_roots(
            inputs.app_name,
            inputs.app_id,
            data_dir,
            cache_dir,
        ))
    }

    pub fn safe_for_last_run(&self) -> bool {
        contains_appcore_marker(&self.data_dir, &self.app_name)
            && contains_appcore_marker(&self.cache_dir, &self.app_name)
    }

    pub fn bins_dir(&self) -> PathBuf {
        self.data_dir.join("bins")
    }

    pub fn security_dir(&self) -> PathBuf {
        self.data_dir.join("security")
    }

    pub fn auth_server_file(&self) -> PathBuf {
        self.bins_dir().join(auth_server_install_name())
    }

    pub fn auth_server_secret_file(&self) -> PathBuf {
        self.security_dir().join("auth-server.secret")
    }

    pub fn auth_transport_secret_file(&self) -> PathBuf {
        self.security_dir().join("auth-transport.secret")
    }

    pub fn auth_required_file(&self) -> PathBuf {
        self.security_dir().join("auth-required.dnt")
    }

    pub fn auth_server_installed(&self) -> bool {
        self.auth_server_file().exists()
    }

    pub fn auth_server_secret_present(&self) -> bool {
        self.auth_server_secret_file().exists()
    }

    pub fn auth_transport_secret_present(&self) -> bool {
        self.auth_transport_secret_file().exists()
    }

    pub fn auth_required(&self) -> bool {
        auth_required_marker_matches(&self.auth_required_file(), &self.app_id)
    }

    fn from_roots(app_name: String, app_id: String, data_dir: PathBuf, cache_dir: PathBuf) -> Self {
        let config_dir = data_dir.join("config");
        Self {
            app_name,
            app_id,
            logs_dir: data_dir.join("logs"),
            runtime_dir: data_dir.join("runtime"),
            backups_dir: data_dir.join("backups"),
            identity_dir: data_dir.join("identity"),
            application_manifest: config_dir.join("application.toml"),
            deployment_manifest: config_dir.join("deployment.toml"),
            secret_file: config_dir.join("runtime.secret"),
            config_dir,
            data_dir,
            cache_dir,
        }
    }
}

pub fn print_paths(paths: &AppCorePaths) {
    println!("app_id: {}", paths.app_id);
    println!("app_name: {}", paths.app_name);
    println!("data_dir: {}", paths.data_dir.display());
    println!("cache_dir: {}", paths.cache_dir.display());
    println!("config_dir: {}", paths.config_dir.display());
    println!("logs_dir: {}", paths.logs_dir.display());
    println!("runtime_dir: {}", paths.runtime_dir.display());
    println!("backups_dir: {}", paths.backups_dir.display());
    println!("identity_dir: {}", paths.identity_dir.display());
    println!("bins_dir: {}", paths.bins_dir().display());
    println!("security_dir: {}", paths.security_dir().display());
    println!(
        "application_manifest: {}",
        paths.application_manifest.display()
    );
    println!(
        "deployment_manifest: {}",
        paths.deployment_manifest.display()
    );
    println!("secret_file: {}", paths.secret_file.display());
    println!("auth_server_file: {}", paths.auth_server_file().display());
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
    println!("auth_server_installed: {}", paths.auth_server_installed());
    println!(
        "auth_server_secret_present: {}",
        paths.auth_server_secret_present()
    );
    println!(
        "auth_transport_secret_present: {}",
        paths.auth_transport_secret_present()
    );
    println!("auth_required: {}", paths.auth_required());
}

#[cfg(target_os = "macos")]
fn data_root(inputs: &PathInputs) -> Result<PathBuf, String> {
    Ok(inputs.data_root.clone().unwrap_or(
        home(inputs)?
            .join("Library/Application Support")
            .join(&inputs.app_name),
    ))
}

#[cfg(target_os = "macos")]
fn cache_root(inputs: &PathInputs) -> Result<PathBuf, String> {
    Ok(inputs
        .cache_root
        .clone()
        .unwrap_or(home(inputs)?.join("Library/Caches").join(&inputs.app_name)))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn data_root(inputs: &PathInputs) -> Result<PathBuf, String> {
    Ok(inputs
        .data_root
        .clone()
        .unwrap_or(home(inputs)?.join(".local/share/appcore-runtime")))
}

#[cfg(all(unix, not(target_os = "macos")))]
fn cache_root(inputs: &PathInputs) -> Result<PathBuf, String> {
    Ok(inputs
        .cache_root
        .clone()
        .unwrap_or(home(inputs)?.join(".cache/appcore-runtime")))
}

#[cfg(windows)]
fn data_root(inputs: &PathInputs) -> Result<PathBuf, String> {
    if let Some(root) = &inputs.data_root {
        return Ok(root.clone());
    }
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .map(|root| root.join(&inputs.app_name))
        .ok_or_else(|| "APPDATA is not set".to_string())
}

#[cfg(windows)]
fn cache_root(inputs: &PathInputs) -> Result<PathBuf, String> {
    if let Some(root) = &inputs.cache_root {
        return Ok(root.clone());
    }
    env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .map(|root| root.join(&inputs.app_name).join("Cache"))
        .ok_or_else(|| "LOCALAPPDATA is not set".to_string())
}

#[cfg(not(windows))]
fn home(inputs: &PathInputs) -> Result<PathBuf, String> {
    inputs
        .home
        .clone()
        .ok_or_else(|| "HOME is not set".to_string())
}

fn contains_appcore_marker(path: &std::path::Path, app_name: &str) -> bool {
    let text = path.to_string_lossy().to_ascii_lowercase();
    text.contains("appcore-runtime")
        || text.contains(&DEFAULT_APP_NAME.to_ascii_lowercase())
        || text.contains(&app_name.to_ascii_lowercase())
        || text.contains("appcore")
}

fn auth_required_marker_matches(path: &std::path::Path, app_id: &str) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    text.contains("\"schema\":\"appcore.auth-required.v1\"")
        && text.contains(&format!("\"app_id\":\"{app_id}\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(root: &str) -> PathInputs {
        PathInputs {
            app_name: "DemoHost".to_string(),
            app_id: "demo".to_string(),
            home: Some(PathBuf::from("/tmp/home")),
            data_root: Some(PathBuf::from(root).join("AppCore-Runtime")),
            cache_root: Some(PathBuf::from(root).join("AppCore-Runtime-cache")),
        }
    }

    #[test]
    fn path_inputs_build_expected_subdirs() {
        let paths = AppCorePaths::from_inputs(inputs("/tmp/appcore-test")).expect("paths");
        assert_eq!(paths.app_id, "demo");
        assert!(paths
            .application_manifest
            .ends_with("config/application.toml"));
        assert!(paths
            .deployment_manifest
            .ends_with("config/deployment.toml"));
        assert!(paths.secret_file.ends_with("config/runtime.secret"));
        assert!(paths.auth_server_file().starts_with(paths.bins_dir()));
        assert!(paths
            .auth_transport_secret_file()
            .starts_with(paths.security_dir()));
        assert!(paths.auth_required_file().starts_with(paths.security_dir()));
    }

    #[test]
    fn last_run_safety_requires_appcore_marker() {
        let paths = AppCorePaths::from_inputs(inputs("/tmp/appcore-test")).expect("paths");
        assert!(paths.safe_for_last_run());
        let unsafe_paths = AppCorePaths::from_roots(
            "DemoHost".to_string(),
            "demo".to_string(),
            PathBuf::from("/tmp/demo"),
            PathBuf::from("/tmp/demo-cache"),
        );
        assert!(!unsafe_paths.safe_for_last_run());
    }

    #[test]
    fn auth_required_requires_matching_marker() {
        let paths = AppCorePaths::from_inputs(inputs("/tmp/appcore-auth")).expect("paths");
        assert!(!paths.auth_required());
        fs::create_dir_all(paths.security_dir()).expect("security dir");
        fs::write(
            paths.auth_required_file(),
            "{\"schema\":\"appcore.auth-required.v1\",\"app_id\":\"demo\"}\n",
        )
        .expect("marker");
        assert!(paths.auth_required());
        let _ = fs::remove_dir_all(paths.data_dir);
    }

    #[test]
    fn auth_status_helpers_do_not_read_secret_material() {
        let paths = AppCorePaths::from_inputs(inputs("/tmp/appcore-auth-status")).expect("paths");
        assert!(!paths.auth_server_installed());
        assert!(!paths.auth_server_secret_present());
        assert!(!paths.auth_transport_secret_present());
        fs::create_dir_all(paths.bins_dir()).expect("bins dir");
        fs::create_dir_all(paths.security_dir()).expect("security dir");
        fs::write(paths.auth_server_file(), b"bin").expect("auth bin");
        fs::write(paths.auth_server_secret_file(), b"secret").expect("secret");
        fs::write(paths.auth_transport_secret_file(), b"pair").expect("pair");
        assert!(paths.auth_server_installed());
        assert!(paths.auth_server_secret_present());
        assert!(paths.auth_transport_secret_present());
        let _ = fs::remove_dir_all(paths.data_dir);
    }
}
