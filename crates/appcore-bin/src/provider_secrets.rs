// =============================================================================
//        #######
//     ###       ###     F: provider_secrets.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Deployment-selected secret provider implementations.

use crate::bootstrap::{now_ms, BootstrapError};
use appcore_contracts::{DeploymentManifestV1, SecretRef};
use appcore_provider::{
    DeploymentProviderPlan, ProviderError, ProviderResult, ResolvedSecret, SecretProvider,
};
#[cfg(windows)]
use appcore_security::WindowsDpapiSecretKeyring;
use appcore_security::{format_secret_material, FileSecretKeyring, HashTokenProvider};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::Path;
use std::sync::Arc;

const MAX_PROVIDER_SECRET_BYTES: u64 = 65_536;

#[derive(Debug, Clone)]
enum SecretBackend {
    EnvFile,
    FileKeyring(Arc<FileSecretKeyring>),
    #[cfg(windows)]
    WindowsDpapiUser(Arc<WindowsDpapiSecretKeyring>),
}

/// Runtime composition adapter for the selected deployment secret provider.
#[derive(Debug, Clone)]
pub(crate) struct DeploymentSecretResolver {
    backend: SecretBackend,
}

impl Default for DeploymentSecretResolver {
    fn default() -> Self {
        Self {
            backend: SecretBackend::EnvFile,
        }
    }
}

impl DeploymentSecretResolver {
    pub(crate) fn from_manifest(manifest: &DeploymentManifestV1) -> Result<Self, BootstrapError> {
        Self::from_config(manifest.secret_provider())
    }

    pub(crate) fn from_plan(plan: &DeploymentProviderPlan) -> Result<Self, BootstrapError> {
        Self::from_config(plan.secret_provider())
    }

    fn from_config(
        config: Option<&appcore_contracts::ProviderConfig>,
    ) -> Result<Self, BootstrapError> {
        let Some(config) = config else {
            return Ok(Self::default());
        };
        match config.provider_id().as_str() {
            "env-file" => Ok(Self::default()),
            "file-keyring-v1" => {
                let root = required_keyring_root(config, "file-keyring-v1")?;
                let keyring = FileSecretKeyring::open(root).map_err(|error| {
                    BootstrapError::Runtime(format!(
                        "file-keyring-v1 initialization failed: {error}"
                    ))
                })?;
                Ok(Self {
                    backend: SecretBackend::FileKeyring(Arc::new(keyring)),
                })
            }
            #[cfg(windows)]
            "windows-dpapi-user-v1" => {
                let root = required_keyring_root(config, "windows-dpapi-user-v1")?;
                let keyring = WindowsDpapiSecretKeyring::open(root).map_err(|error| {
                    BootstrapError::Runtime(format!(
                        "windows-dpapi-user-v1 initialization failed: {error}"
                    ))
                })?;
                Ok(Self {
                    backend: SecretBackend::WindowsDpapiUser(Arc::new(keyring)),
                })
            }
            provider => Err(BootstrapError::Runtime(format!(
                "deployment selected unavailable secret provider: {provider}"
            ))),
        }
    }

    pub(crate) fn rotating_hash_token_provider(
        &self,
        reference: &SecretRef,
        salts: Vec<Vec<u8>>,
    ) -> Result<Option<HashTokenProvider>, BootstrapError> {
        match &self.backend {
            SecretBackend::EnvFile => Ok(None),
            SecretBackend::FileKeyring(keyring) => {
                require_active_reference(reference, "file-keyring-v1")?;
                HashTokenProvider::from_keyring(keyring.as_ref().clone(), salts)
                    .map(Some)
                    .map_err(|_| unavailable_active_keyring())
            }
            #[cfg(windows)]
            SecretBackend::WindowsDpapiUser(keyring) => {
                require_active_reference(reference, "windows-dpapi-user-v1")?;
                HashTokenProvider::from_windows_dpapi_keyring(keyring.as_ref().clone(), salts)
                    .map(Some)
                    .map_err(|_| unavailable_active_keyring())
            }
        }
    }
}

impl SecretProvider for DeploymentSecretResolver {
    fn resolve(&self, reference: &SecretRef) -> ProviderResult<ResolvedSecret> {
        match &self.backend {
            SecretBackend::EnvFile => resolve_env_file(reference),
            SecretBackend::FileKeyring(keyring) if reference.as_str().starts_with("provider:") => {
                resolve_keyring(keyring, reference)
            }
            SecretBackend::FileKeyring(_) => resolve_env_file(reference),
            #[cfg(windows)]
            SecretBackend::WindowsDpapiUser(keyring)
                if reference.as_str().starts_with("provider:") =>
            {
                resolve_windows_dpapi_keyring(keyring, reference)
            }
            #[cfg(windows)]
            SecretBackend::WindowsDpapiUser(_) => resolve_env_file(reference),
        }
    }
}

fn required_keyring_root<'a>(
    config: &'a appcore_contracts::ProviderConfig,
    provider: &str,
) -> Result<&'a str, BootstrapError> {
    config
        .settings()
        .get("root")
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| BootstrapError::Runtime(format!("{provider} requires settings.root")))
}

fn require_active_reference(reference: &SecretRef, provider: &str) -> Result<(), BootstrapError> {
    if reference.as_str() == "provider:active" {
        Ok(())
    } else {
        Err(BootstrapError::Runtime(format!(
            "{provider} runtime security requires provider:active"
        )))
    }
}

fn unavailable_active_keyring() -> BootstrapError {
    BootstrapError::Runtime(
        "active keyring material is unavailable for runtime security".to_string(),
    )
}

fn resolve_env_file(reference: &SecretRef) -> ProviderResult<ResolvedSecret> {
    let value = reference.as_str();
    if let Some(name) = value.strip_prefix("env:") {
        validate_environment_name(name)?;
        return std::env::var(name)
            .map_err(|_| ProviderError::SecretUnavailable(format!("environment:{name}")))
            .and_then(ResolvedSecret::new);
    }
    if let Some(path) = value.strip_prefix("file:") {
        return read_private_secret(Path::new(path)).and_then(ResolvedSecret::new);
    }
    Err(ProviderError::SecretUnavailable(
        "env-file accepts only env: or file: references".to_string(),
    ))
}

fn resolve_keyring(
    keyring: &FileSecretKeyring,
    reference: &SecretRef,
) -> ProviderResult<ResolvedSecret> {
    if reference.as_str() != "provider:active" {
        return Err(ProviderError::SecretUnavailable(
            "file-keyring-v1 accepts only provider:active".to_string(),
        ));
    }
    let material = keyring.resolve_active(now_ms()).map_err(|error| {
        ProviderError::SecretUnavailable(format!("active keyring material unavailable: {error}"))
    })?;
    ResolvedSecret::new(format_secret_material(&material))
}

#[cfg(windows)]
fn resolve_windows_dpapi_keyring(
    keyring: &WindowsDpapiSecretKeyring,
    reference: &SecretRef,
) -> ProviderResult<ResolvedSecret> {
    if reference.as_str() != "provider:active" {
        return Err(ProviderError::SecretUnavailable(
            "windows-dpapi-user-v1 accepts only provider:active".to_string(),
        ));
    }
    let material = keyring.resolve_active(now_ms()).map_err(|error| {
        ProviderError::SecretUnavailable(format!("active keyring material unavailable: {error}"))
    })?;
    ResolvedSecret::new(format_secret_material(&material))
}

fn validate_environment_name(name: &str) -> ProviderResult<()> {
    if name.is_empty()
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        return Err(ProviderError::SecretUnavailable(
            "invalid environment secret reference".to_string(),
        ));
    }
    Ok(())
}

fn read_private_secret(path: &Path) -> ProviderResult<String> {
    reject_unsafe_path(path)?;
    let mut file = open_no_follow(path)?;
    validate_private_file(&file)?;
    let length = file
        .metadata()
        .map_err(|_| unavailable("referenced file metadata unavailable"))?
        .len();
    if length == 0 || length > MAX_PROVIDER_SECRET_BYTES {
        return Err(unavailable(
            "referenced file is empty or exceeds the secret size limit",
        ));
    }
    let mut contents = String::with_capacity(length as usize);
    file.read_to_string(&mut contents)
        .map_err(|_| unavailable("referenced file is unreadable or not UTF-8"))?;
    let contents = contents.trim().to_string();
    if contents.is_empty() {
        return Err(unavailable("referenced file is empty"));
    }
    Ok(contents)
}

fn reject_unsafe_path(path: &Path) -> ProviderResult<()> {
    let metadata =
        fs::symlink_metadata(path).map_err(|_| unavailable("referenced file is unavailable"))?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(unavailable("referenced path is not a regular file"));
    }
    Ok(())
}

#[cfg(unix)]
fn open_no_follow(path: &Path) -> ProviderResult<File> {
    use std::os::unix::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|_| unavailable("referenced file cannot be opened safely"))
}

#[cfg(not(unix))]
fn open_no_follow(path: &Path) -> ProviderResult<File> {
    OpenOptions::new()
        .read(true)
        .open(path)
        .map_err(|_| unavailable("referenced file cannot be opened safely"))
}

#[cfg(unix)]
fn validate_private_file(file: &File) -> ProviderResult<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let metadata = file
        .metadata()
        .map_err(|_| unavailable("referenced file metadata unavailable"))?;
    if !metadata.is_file()
        || metadata.permissions().mode() & 0o077 != 0
        || metadata.uid() != unsafe { libc::geteuid() }
    {
        return Err(unavailable(
            "referenced file must be owner-only and owned by the runtime user",
        ));
    }
    Ok(())
}

#[cfg(not(unix))]
fn validate_private_file(file: &File) -> ProviderResult<()> {
    if !file
        .metadata()
        .map_err(|_| unavailable("referenced file metadata unavailable"))?
        .is_file()
    {
        return Err(unavailable("referenced path is not a regular file"));
    }
    Ok(())
}

fn unavailable(message: &str) -> ProviderError {
    ProviderError::SecretUnavailable(message.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use appcore_contracts::{
        ApplicationId, InstallationId, NetworkConfig, ProviderConfig, ProviderId, RuntimeMode,
    };
    use appcore_security::{
        new_rotated_secret, parse_secret_material, SecuritySecretMaterial, SecuritySecretStatus,
    };

    #[test]
    fn file_keyring_provider_resolves_only_the_active_material() {
        let root = std::env::temp_dir().join(format!(
            "appcore-provider-keyring-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let keyring = FileSecretKeyring::open(&root).unwrap();
        let material = new_rotated_secret(None).unwrap();
        keyring.install_initial(&material).unwrap();
        let keyring_config = ProviderConfig::new(ProviderId::new("file-keyring-v1").unwrap())
            .with_setting("root", root.to_string_lossy())
            .unwrap();
        let deployment = DeploymentManifestV1::builder(
            InstallationId::new("keyring-install").unwrap(),
            ApplicationId::new("keyring-app").unwrap(),
            RuntimeMode::Standalone,
            ProviderConfig::new(ProviderId::new("file").unwrap()),
            NetworkConfig::new(
                ProviderId::new("http").unwrap(),
                ProviderId::new("http").unwrap(),
            ),
        )
        .with_secret_provider(keyring_config)
        .with_secret(
            "runtime_security",
            SecretRef::new("provider:active").unwrap(),
        )
        .unwrap()
        .build()
        .unwrap();
        let resolver = DeploymentSecretResolver::from_manifest(&deployment).unwrap();
        let resolved = resolver
            .resolve(deployment.secrets().get("runtime_security").unwrap())
            .unwrap();
        let parsed = parse_secret_material(resolved.expose().as_bytes()).unwrap();

        assert_eq!(parsed.metadata.key_id, material.metadata.key_id);
        assert_eq!(parsed.metadata.status, SecuritySecretStatus::Active);
        assert_eq!(parsed.secret, material.secret);
        assert!(matches!(
            resolver.resolve(&SecretRef::new("file:/tmp/secret").unwrap()),
            Err(ProviderError::SecretUnavailable(_))
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn file_keyring_provider_rejects_revoked_active_key() {
        let root = std::env::temp_dir().join(format!(
            "appcore-provider-keyring-revoked-{}-{}",
            std::process::id(),
            now_ms()
        ));
        let keyring = FileSecretKeyring::open(&root).unwrap();
        let material: SecuritySecretMaterial = new_rotated_secret(None).unwrap();
        let key_id = material.metadata.key_id.clone();
        keyring.install_initial(&material).unwrap();
        keyring.revoke(&key_id).unwrap();
        let config = ProviderConfig::new(ProviderId::new("file-keyring-v1").unwrap())
            .with_setting("root", root.to_string_lossy())
            .unwrap();
        let deployment = DeploymentManifestV1::builder(
            InstallationId::new("keyring-revoked").unwrap(),
            ApplicationId::new("keyring-app").unwrap(),
            RuntimeMode::Standalone,
            ProviderConfig::new(ProviderId::new("file").unwrap()),
            NetworkConfig::new(
                ProviderId::new("http").unwrap(),
                ProviderId::new("http").unwrap(),
            ),
        )
        .with_secret_provider(config)
        .build()
        .unwrap();
        let resolver = DeploymentSecretResolver::from_manifest(&deployment).unwrap();

        assert!(matches!(
            resolver.resolve(&SecretRef::new("provider:active").unwrap()),
            Err(ProviderError::SecretUnavailable(_))
        ));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[cfg(not(windows))]
    #[test]
    fn windows_dpapi_provider_fails_closed_on_other_platforms() {
        let config = ProviderConfig::new(ProviderId::new("windows-dpapi-user-v1").unwrap())
            .with_setting("root", "/tmp/appcore-dpapi")
            .unwrap();
        let deployment = DeploymentManifestV1::builder(
            InstallationId::new("dpapi-unavailable").unwrap(),
            ApplicationId::new("keyring-app").unwrap(),
            RuntimeMode::Standalone,
            ProviderConfig::new(ProviderId::new("file").unwrap()),
            NetworkConfig::new(
                ProviderId::new("http").unwrap(),
                ProviderId::new("http").unwrap(),
            ),
        )
        .with_secret_provider(config)
        .build()
        .unwrap();

        let result = DeploymentSecretResolver::from_manifest(&deployment);
        assert!(matches!(result, Err(BootstrapError::Runtime(_))));
    }
}
