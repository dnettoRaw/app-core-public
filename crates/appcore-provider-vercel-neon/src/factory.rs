// =============================================================================
//        #######
//     ###       ###     F: factory.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use appcore_control_plane::{
    BearerHttpTransport, ControlPlaneHttpConfig, ControlPlaneProvider, HttpControlPlaneClient,
    RetryPolicy, SecretString,
};
use appcore_provider::{
    ProviderContext, ProviderError, ProviderFactory, ProviderResult, ProviderRole, SecretProvider,
};
use std::sync::Arc;

/// Provider ID selected in a deployment manifest.
pub const VERCEL_NEON_PROVIDER_ID: &str = "vercel-neon";
/// Required provider secret reference containing the Vercel API bearer token.
pub const AUTH_TOKEN_SECRET: &str = "auth_token";

/// Shared control-plane interface produced by provider factories.
pub type SharedControlPlaneProvider = Arc<dyn ControlPlaneProvider>;

/// Factory for the official Vercel-hosted, Neon-backed control plane.
#[derive(Debug, Clone, Copy, Default)]
pub struct VercelNeonControlPlaneFactory;

impl ProviderFactory<SharedControlPlaneProvider> for VercelNeonControlPlaneFactory {
    fn role(&self) -> ProviderRole {
        ProviderRole::ControlPlane
    }

    fn provider_id(&self) -> &'static str {
        VERCEL_NEON_PROVIDER_ID
    }

    fn create(
        &self,
        config: &appcore_contracts::ProviderConfig,
        context: &ProviderContext,
        secrets: &dyn SecretProvider,
    ) -> ProviderResult<SharedControlPlaneProvider> {
        if context.runtime_mode() != appcore_contracts::RuntimeMode::Cluster {
            return Err(ProviderError::InvalidConfiguration(
                "vercel-neon control plane requires cluster mode".to_string(),
            ));
        }
        let endpoint = config.endpoint().ok_or_else(|| {
            ProviderError::InvalidConfiguration(
                "vercel-neon control plane requires an HTTPS endpoint".to_string(),
            )
        })?;
        if !endpoint.starts_with("https://") {
            return Err(ProviderError::InvalidConfiguration(
                "vercel-neon control plane endpoint must use HTTPS".to_string(),
            ));
        }
        let token_ref = config.secret_refs().get(AUTH_TOKEN_SECRET).ok_or_else(|| {
            ProviderError::InvalidConfiguration(format!(
                "vercel-neon requires secret_refs.{AUTH_TOKEN_SECRET}"
            ))
        })?;
        let token = secrets.resolve(token_ref)?;
        let timeout_ms = parse_u64_setting(config, "timeout_ms", 5_000)?;
        let max_attempts = parse_usize_setting(config, "max_attempts", 3)?;
        let transport =
            BearerHttpTransport::from_secret(SecretString::from_zeroizing(token.into_zeroizing()));
        Ok(Arc::new(HttpControlPlaneClient::new(
            ControlPlaneHttpConfig {
                base_url: endpoint.to_string(),
                timeout_ms,
                retry_policy: RetryPolicy {
                    max_attempts,
                    initial_backoff_ms: parse_u64_setting(config, "initial_backoff_ms", 100)?,
                    max_backoff_ms: parse_u64_setting(config, "max_backoff_ms", 1_000)?,
                },
            },
            transport,
        )))
    }
}

fn parse_u64_setting(
    config: &appcore_contracts::ProviderConfig,
    name: &str,
    default: u64,
) -> ProviderResult<u64> {
    let Some(value) = config.settings().get(name) else {
        return Ok(default);
    };
    let parsed = value.parse::<u64>().map_err(|_| {
        ProviderError::InvalidConfiguration(format!(
            "vercel-neon setting {name} must be an unsigned integer"
        ))
    })?;
    if parsed == 0 {
        return Err(ProviderError::InvalidConfiguration(format!(
            "vercel-neon setting {name} must be greater than zero"
        )));
    }
    Ok(parsed)
}

fn parse_usize_setting(
    config: &appcore_contracts::ProviderConfig,
    name: &str,
    default: usize,
) -> ProviderResult<usize> {
    let parsed = parse_u64_setting(config, name, default as u64)?;
    usize::try_from(parsed).map_err(|_| {
        ProviderError::InvalidConfiguration(format!(
            "vercel-neon setting {name} exceeds this platform"
        ))
    })
}
