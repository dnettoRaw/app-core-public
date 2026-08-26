// =============================================================================
//        #######
//     ###       ###     F: provider_factories.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Runtime provider factories selected by the deployment plan.

use super::require_https_for_remote_endpoint;
use appcore_contracts::ProviderConfig;
use appcore_control_plane::{
    BearerHttpTransport, ControlPlaneHttpConfig, FileControlPlane, HttpControlPlaneClient,
    InMemoryControlPlane, PooledHttpTransport, RetryPolicy, SecretString,
};
use appcore_provider::{
    FileCoordinationStore, ProviderContext, ProviderError, ProviderFactory, ProviderResult,
    ProviderRole, SecretProvider, SharedCoordinationStore,
};
use appcore_provider_vercel_neon::SharedControlPlaneProvider;
use std::sync::Arc;

#[derive(Debug, Clone, Copy)]
pub(super) struct HttpControlPlaneFactory;

impl ProviderFactory<SharedControlPlaneProvider> for HttpControlPlaneFactory {
    fn role(&self) -> ProviderRole {
        ProviderRole::ControlPlane
    }

    fn provider_id(&self) -> &'static str {
        "http-control-plane"
    }

    fn create(
        &self,
        config: &ProviderConfig,
        _context: &ProviderContext,
        secrets: &dyn SecretProvider,
    ) -> ProviderResult<SharedControlPlaneProvider> {
        let endpoint = config.endpoint().ok_or_else(|| {
            ProviderError::InvalidConfiguration(
                "http control plane requires an endpoint".to_string(),
            )
        })?;
        require_https_for_remote_endpoint(endpoint)?;
        let http_config = ControlPlaneHttpConfig {
            base_url: endpoint.to_string(),
            timeout_ms: parse_u64_setting(config, "timeout_ms", 5_000)?,
            retry_policy: RetryPolicy {
                max_attempts: parse_usize_setting(config, "max_attempts", 2)?,
                initial_backoff_ms: parse_u64_setting(config, "initial_backoff_ms", 100)?,
                max_backoff_ms: parse_u64_setting(config, "max_backoff_ms", 1_000)?,
            },
        };
        match config.secret_refs().get("auth_token") {
            Some(reference) => {
                let token = secrets.resolve(reference)?;
                Ok(Arc::new(HttpControlPlaneClient::new(
                    http_config,
                    BearerHttpTransport::from_secret(SecretString::from_zeroizing(
                        token.into_zeroizing(),
                    )),
                )))
            }
            None => Ok(Arc::new(HttpControlPlaneClient::new(
                http_config,
                PooledHttpTransport::default(),
            ))),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct InMemoryControlPlaneFactory;

impl ProviderFactory<SharedControlPlaneProvider> for InMemoryControlPlaneFactory {
    fn role(&self) -> ProviderRole {
        ProviderRole::ControlPlane
    }

    fn provider_id(&self) -> &'static str {
        "in-memory"
    }

    fn create(
        &self,
        _config: &ProviderConfig,
        _context: &ProviderContext,
        _secrets: &dyn SecretProvider,
    ) -> ProviderResult<SharedControlPlaneProvider> {
        Ok(Arc::new(InMemoryControlPlane::default()))
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FileControlPlaneFactory;

impl ProviderFactory<SharedControlPlaneProvider> for FileControlPlaneFactory {
    fn role(&self) -> ProviderRole {
        ProviderRole::ControlPlane
    }

    fn provider_id(&self) -> &'static str {
        "file-control-plane"
    }

    fn create(
        &self,
        config: &ProviderConfig,
        _context: &ProviderContext,
        _secrets: &dyn SecretProvider,
    ) -> ProviderResult<SharedControlPlaneProvider> {
        let path = required_setting(config, "path")?;
        let retention_ms = parse_u64_setting(config, "retention_ms", 86_400_000)?;
        FileControlPlane::open(path, retention_ms)
            .map(|control| Arc::new(control) as SharedControlPlaneProvider)
            .map_err(|error| ProviderError::Initialization(error.to_string()))
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct LocalMeshControlPlaneFactory;

impl ProviderFactory<SharedControlPlaneProvider> for LocalMeshControlPlaneFactory {
    fn role(&self) -> ProviderRole {
        ProviderRole::ControlPlane
    }

    fn provider_id(&self) -> &'static str {
        "local-mesh"
    }

    fn create(
        &self,
        config: &ProviderConfig,
        _context: &ProviderContext,
        _secrets: &dyn SecretProvider,
    ) -> ProviderResult<SharedControlPlaneProvider> {
        let path = required_setting(config, "path")?;
        let retention_ms = parse_u64_setting(config, "retention_ms", 86_400_000)?;
        FileControlPlane::open(path, retention_ms)
            .map(|control| Arc::new(control) as SharedControlPlaneProvider)
            .map_err(|error| ProviderError::Initialization(error.to_string()))
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) struct FileCoordinationStoreFactory;

impl ProviderFactory<SharedCoordinationStore> for FileCoordinationStoreFactory {
    fn role(&self) -> ProviderRole {
        ProviderRole::CoordinationStore
    }

    fn provider_id(&self) -> &'static str {
        "file-coordination-v2"
    }

    fn create(
        &self,
        config: &ProviderConfig,
        _context: &ProviderContext,
        _secrets: &dyn SecretProvider,
    ) -> ProviderResult<SharedCoordinationStore> {
        FileCoordinationStore::open(required_setting(config, "path")?)
            .map(|store| Arc::new(store) as SharedCoordinationStore)
    }
}

fn parse_u64_setting(config: &ProviderConfig, name: &str, default: u64) -> ProviderResult<u64> {
    let Some(value) = config.settings().get(name) else {
        return Ok(default);
    };
    let parsed = value.parse::<u64>().map_err(|_| {
        ProviderError::InvalidConfiguration(format!(
            "provider setting {name} must be an unsigned integer"
        ))
    })?;
    if parsed == 0 {
        return Err(ProviderError::InvalidConfiguration(format!(
            "provider setting {name} must be greater than zero"
        )));
    }
    Ok(parsed)
}

pub(super) fn required_setting<'a>(
    config: &'a ProviderConfig,
    name: &str,
) -> ProviderResult<&'a str> {
    config
        .settings()
        .get(name)
        .map(String::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| {
            ProviderError::InvalidConfiguration(format!("provider setting {name} is required"))
        })
}

fn parse_usize_setting(
    config: &ProviderConfig,
    name: &str,
    default: usize,
) -> ProviderResult<usize> {
    usize::try_from(parse_u64_setting(config, name, default as u64)?).map_err(|_| {
        ProviderError::InvalidConfiguration(format!(
            "provider setting {name} exceeds this platform"
        ))
    })
}
