// =============================================================================
//        #######
//     ###       ###     F: authenticity.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/24 11:51:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Update artifact authenticity policy construction.

use crate::bootstrap::BootstrapError;
use appcore_contracts::ProviderConfig;
use appcore_update::{
    ArtifactAuthenticityVerifier, ArtifactTrustPolicy, Ed25519ArtifactVerifier,
    PolicyArtifactVerifier,
};
use std::sync::Arc;

pub(super) struct AuthenticitySelection {
    pub(super) verifier: Arc<dyn ArtifactAuthenticityVerifier>,
    pub(super) unsigned_local_artifacts: bool,
}

pub(super) fn build_authenticity_verifier(
    config: &ProviderConfig,
) -> Result<AuthenticitySelection, BootstrapError> {
    if config.settings().contains_key("trusted_local") {
        return Err(BootstrapError::Runtime(
            "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
        ));
    }
    if parse_bool_setting(config, "allow_unsigned_local_artifacts", false)? {
        return unsigned_local_verifier(config);
    }

    let signature = signature_verifier(config)?;
    let policy = trust_policy(config)?;
    Ok(AuthenticitySelection {
        verifier: Arc::new(PolicyArtifactVerifier::new(policy, signature)),
        unsigned_local_artifacts: false,
    })
}

fn signature_verifier(config: &ProviderConfig) -> Result<Ed25519ArtifactVerifier, BootstrapError> {
    let mut signature = Ed25519ArtifactVerifier::new();
    for (name, value) in config
        .settings()
        .iter()
        .filter(|(name, _)| name.starts_with("signing_key."))
    {
        signature
            .add_trust_root_hex(name.trim_start_matches("signing_key."), value)
            .map_err(update_bootstrap_error)?;
    }
    if signature.trust_root_count() == 0 {
        return Err(BootstrapError::Runtime(
            "automatic updates require at least one signing_key.<id> and non-empty allowlists"
                .to_string(),
        ));
    }
    Ok(signature)
}

fn trust_policy(config: &ProviderConfig) -> Result<ArtifactTrustPolicy, BootstrapError> {
    let mut policy = ArtifactTrustPolicy::new();
    for channel in comma_separated_setting(config, "allowed_channels")? {
        policy = policy
            .allow_channel(channel)
            .map_err(update_bootstrap_error)?;
    }
    for origin in comma_separated_setting(config, "allowed_origins")? {
        policy = policy
            .allow_origin(origin)
            .map_err(update_bootstrap_error)?;
    }
    Ok(policy)
}

#[cfg(feature = "allow-unsigned-local-artifacts")]
fn unsigned_local_verifier(
    config: &ProviderConfig,
) -> Result<AuthenticitySelection, BootstrapError> {
    let root = config
        .settings()
        .get("unsigned_artifact_root")
        .ok_or_else(|| {
            BootstrapError::Runtime(
                "unsigned local artifacts require `unsigned_artifact_root`".to_string(),
            )
        })?;
    let verifier =
        appcore_update::UnsignedLocalArtifactVerifier::new(root).map_err(update_bootstrap_error)?;
    Ok(AuthenticitySelection {
        verifier: Arc::new(verifier),
        unsigned_local_artifacts: true,
    })
}

#[cfg(not(feature = "allow-unsigned-local-artifacts"))]
fn unsigned_local_verifier(
    _config: &ProviderConfig,
) -> Result<AuthenticitySelection, BootstrapError> {
    Err(BootstrapError::Runtime(
        "allow_unsigned_local_artifacts requires the compile-time \
         `allow-unsigned-local-artifacts` feature"
            .to_string(),
    ))
}

fn parse_bool_setting(
    config: &ProviderConfig,
    name: &str,
    default: bool,
) -> Result<bool, BootstrapError> {
    let Some(value) = config.settings().get(name) else {
        return Ok(default);
    };
    value.parse::<bool>().map_err(|_| {
        BootstrapError::Runtime(format!("update provider setting `{name}` must be a bool"))
    })
}

fn comma_separated_setting(
    config: &ProviderConfig,
    name: &str,
) -> Result<Vec<String>, BootstrapError> {
    let value = config.settings().get(name).ok_or_else(|| {
        BootstrapError::Runtime(format!("update provider setting `{name}` is required"))
    })?;
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if values.is_empty() {
        return Err(BootstrapError::Runtime(format!(
            "update provider setting `{name}` must not be empty"
        )));
    }
    Ok(values)
}

fn update_bootstrap_error(error: appcore_update::UpdateError) -> BootstrapError {
    BootstrapError::Runtime(format!("invalid update authenticity policy: {error}"))
}
