// =============================================================================
//        #######
//     ###       ###     F: security_cli.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/02 13:08:16 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Owns token and secret-management CLI commands.

use crate::bootstrap::{load_config, load_security_provider, now_ms, BootstrapError};
#[cfg(windows)]
use appcore_security::WindowsDpapiSecretKeyring;
use appcore_security::{
    format_secret_material, new_rotated_secret, parse_secret_material, CommandTokenFactory,
    FileSecretKeyring, TokenClaims, DEFAULT_RUNTIME_TOKEN_TTL_MS, LOCAL_ADMIN_SUBJECT,
};
use std::fs;

pub(crate) fn run_token_command(
    config_path: Option<&str>,
    command_name: Option<&str>,
    scope: Option<&str>,
    subject: Option<&str>,
    ttl_ms: Option<u64>,
) -> Result<(), BootstrapError> {
    let config = load_config(config_path)?;
    let provider = load_security_provider(&config)?.provider;
    let ttl_ms = ttl_ms
        .or(config.token_ttl_ms)
        .unwrap_or(DEFAULT_RUNTIME_TOKEN_TTL_MS);
    let subject = wildcard_subject(scope, subject);
    let claims = TokenClaims {
        issuer: config.token_issuer,
        audience: config.token_audience,
        salt: "command".to_string(),
        ttl_ms,
    };
    let factory = CommandTokenFactory::new(&provider, claims);
    let command_name = if scope == Some("*") {
        command_name
    } else {
        Some(command_name.unwrap_or("runtime.ping"))
    };
    let token = factory
        .create_v1_scoped(command_name, scope, subject, now_ms(), ttl_ms)
        .map_err(|_| BootstrapError::Runtime("token generation failed".to_string()))?;
    println!("{token}");
    Ok(())
}

pub(crate) fn run_token_sync(
    config_path: Option<&str>,
    subject: Option<&str>,
    ttl_ms: Option<u64>,
) -> Result<(), BootstrapError> {
    let config = load_config(config_path)?;
    let provider = load_security_provider(&config)?.provider;
    let ttl_ms = ttl_ms
        .or(config.token_ttl_ms)
        .unwrap_or(DEFAULT_RUNTIME_TOKEN_TTL_MS);
    let claims = TokenClaims {
        issuer: config.token_issuer,
        audience: config.token_audience,
        salt: "sync".to_string(),
        ttl_ms,
    };
    let factory = CommandTokenFactory::new(&provider, claims);
    let token = factory
        .create_v1_for_purpose("sync", None, subject, now_ms(), ttl_ms)
        .map_err(|_| BootstrapError::Runtime("token generation failed".to_string()))?;
    println!("{token}");
    Ok(())
}

pub(crate) fn run_token_query(
    config_path: Option<&str>,
    query_name: Option<&str>,
    scope: Option<&str>,
    subject: Option<&str>,
    ttl_ms: Option<u64>,
) -> Result<(), BootstrapError> {
    let config = load_config(config_path)?;
    let provider = load_security_provider(&config)?.provider;
    let ttl_ms = ttl_ms
        .or(config.token_ttl_ms)
        .unwrap_or(DEFAULT_RUNTIME_TOKEN_TTL_MS);
    let subject = wildcard_subject(scope, subject);
    let claims = TokenClaims {
        issuer: config.token_issuer,
        audience: config.token_audience,
        salt: "query".to_string(),
        ttl_ms,
    };
    let factory = CommandTokenFactory::new(&provider, claims);
    let query_name = if scope == Some("*") {
        query_name
    } else {
        Some(query_name.unwrap_or("runtime.status"))
    };
    let token = factory
        .create_v1_for_purpose_scoped("query", query_name, scope, subject, now_ms(), ttl_ms)
        .map_err(|_| BootstrapError::Runtime("token generation failed".to_string()))?;
    println!("{token}");
    Ok(())
}

pub(crate) fn run_security_secret_status(config_path: Option<&str>) -> Result<(), BootstrapError> {
    let config = load_config(config_path)?;
    let raw = fs::read(&config.security_secret_path)
        .map_err(|_| BootstrapError::Runtime("security secret load failed".to_string()))?;
    let material = parse_secret_material(&raw)
        .map_err(|_| BootstrapError::Runtime("security secret format invalid".to_string()))?;
    let expired = material.is_expired(now_ms());
    println!("key_id: {}", material.metadata.key_id);
    println!("created_at_ms: {}", material.metadata.created_at_ms);
    println!(
        "expires_at_ms: {}",
        material
            .metadata
            .expires_at_ms
            .map(|v| v.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    println!("status: {}", material.metadata.status.as_str());
    println!("expired: {}", expired);
    Ok(())
}

pub(crate) fn run_security_secret_rotate(
    config_path: Option<&str>,
    out: &str,
) -> Result<(), BootstrapError> {
    let config = load_config(config_path)?;
    let ttl_ms = config.token_ttl_ms.unwrap_or(DEFAULT_RUNTIME_TOKEN_TTL_MS);
    let expires_at_ms = Some(now_ms().saturating_add(ttl_ms));
    let material = new_rotated_secret(expires_at_ms)
        .map_err(|_| BootstrapError::Runtime("security secret generation failed".to_string()))?;
    let text = format_secret_material(&material);
    fs::write(out, text)
        .map_err(|_| BootstrapError::Runtime("failed to write rotated secret".to_string()))?;
    println!("{out}");
    Ok(())
}

pub(crate) fn run_security_keyring_init(
    root: &str,
    provider: &str,
    ttl_ms: Option<u64>,
) -> Result<(), BootstrapError> {
    let keyring = OperationalKeyring::open(root, provider)?;
    let material = new_keyring_material(ttl_ms)?;
    keyring.install_initial(&material).map_err(keyring_error)?;
    println!("key_id: {}", material.metadata.key_id);
    Ok(())
}

pub(crate) fn run_security_keyring_rotate(
    root: &str,
    provider: &str,
    ttl_ms: Option<u64>,
) -> Result<(), BootstrapError> {
    let keyring = OperationalKeyring::open(root, provider)?;
    let material = new_keyring_material(ttl_ms)?;
    let previous = keyring.rotate(&material, now_ms()).map_err(keyring_error)?;
    println!("key_id: {}", material.metadata.key_id);
    println!("previous_key_id: {}", previous.as_deref().unwrap_or("none"));
    Ok(())
}

pub(crate) fn run_security_keyring_revoke(
    root: &str,
    provider: &str,
    key_id: &str,
) -> Result<(), BootstrapError> {
    let keyring = OperationalKeyring::open(root, provider)?;
    keyring.revoke(key_id).map_err(keyring_error)?;
    println!("revoked_key_id: {key_id}");
    Ok(())
}

pub(crate) fn run_security_keyring_status(
    root: &str,
    provider: &str,
) -> Result<(), BootstrapError> {
    let keyring = OperationalKeyring::open(root, provider)?;
    let material = keyring.resolve_active(now_ms()).map_err(keyring_error)?;
    println!("format: {}", keyring.format());
    println!("active_key_id: {}", material.metadata.key_id);
    println!(
        "expires_at_ms: {}",
        material
            .metadata
            .expires_at_ms
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".to_string())
    );
    Ok(())
}

pub(crate) fn run_security_keyring_recover(
    root: &str,
    provider: &str,
) -> Result<(), BootstrapError> {
    let keyring = OperationalKeyring::open(root, provider)?;
    let key_id = keyring.recover(now_ms()).map_err(keyring_error)?;
    println!("recovered_key_id: {key_id}");
    Ok(())
}

enum OperationalKeyring {
    File(FileSecretKeyring),
    #[cfg(windows)]
    WindowsDpapiUser(WindowsDpapiSecretKeyring),
}

impl OperationalKeyring {
    fn open(root: &str, provider: &str) -> Result<Self, BootstrapError> {
        match provider {
            "file-keyring-v1" => FileSecretKeyring::open(root)
                .map(Self::File)
                .map_err(keyring_error),
            #[cfg(windows)]
            "windows-dpapi-user-v1" => WindowsDpapiSecretKeyring::open(root)
                .map(Self::WindowsDpapiUser)
                .map_err(keyring_error),
            _ => Err(BootstrapError::Runtime(format!(
                "security keyring provider is unavailable: {provider}"
            ))),
        }
    }

    fn install_initial(
        &self,
        material: &appcore_security::SecuritySecretMaterial,
    ) -> appcore_security::SecretAccessResult<()> {
        match self {
            Self::File(keyring) => keyring.install_initial(material),
            #[cfg(windows)]
            Self::WindowsDpapiUser(keyring) => keyring.install_initial(material),
        }
    }

    fn rotate(
        &self,
        material: &appcore_security::SecuritySecretMaterial,
        now_ms: u64,
    ) -> appcore_security::SecretAccessResult<Option<String>> {
        match self {
            Self::File(keyring) => keyring.rotate(material, now_ms),
            #[cfg(windows)]
            Self::WindowsDpapiUser(keyring) => keyring.rotate(material, now_ms),
        }
    }

    fn revoke(&self, key_id: &str) -> appcore_security::SecretAccessResult<()> {
        match self {
            Self::File(keyring) => keyring.revoke(key_id),
            #[cfg(windows)]
            Self::WindowsDpapiUser(keyring) => keyring.revoke(key_id),
        }
    }

    fn resolve_active(
        &self,
        now_ms: u64,
    ) -> appcore_security::SecretAccessResult<appcore_security::SecuritySecretMaterial> {
        match self {
            Self::File(keyring) => keyring.resolve_active(now_ms),
            #[cfg(windows)]
            Self::WindowsDpapiUser(keyring) => keyring.resolve_active(now_ms),
        }
    }

    fn recover(&self, now_ms: u64) -> appcore_security::SecretAccessResult<String> {
        match self {
            Self::File(keyring) => keyring.recover(now_ms),
            #[cfg(windows)]
            Self::WindowsDpapiUser(keyring) => keyring.recover(now_ms),
        }
    }

    fn format(&self) -> &'static str {
        match self {
            Self::File(_) => appcore_security::FILE_SECRET_KEYRING_FORMAT,
            #[cfg(windows)]
            Self::WindowsDpapiUser(_) => appcore_security::WINDOWS_DPAPI_USER_SECRET_KEYRING_FORMAT,
        }
    }
}

fn new_keyring_material(
    ttl_ms: Option<u64>,
) -> Result<appcore_security::SecuritySecretMaterial, BootstrapError> {
    let expires_at_ms = ttl_ms.map(|ttl| now_ms().saturating_add(ttl.max(1)));
    new_rotated_secret(expires_at_ms)
        .map_err(|_| BootstrapError::Runtime("security secret generation failed".to_string()))
}

fn keyring_error(error: impl std::fmt::Display) -> BootstrapError {
    BootstrapError::Runtime(format!("security keyring operation failed: {error}"))
}

fn wildcard_subject<'a>(scope: Option<&str>, subject: Option<&'a str>) -> Option<&'a str> {
    if scope == Some("*") {
        return Some(LOCAL_ADMIN_SUBJECT);
    }
    subject
}
