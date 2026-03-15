// =============================================================================
//        #######
//     ###       ###     F: application.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 10:59:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::manifest_error;
use crate::bootstrap::BootstrapError;
use crate::runtime_config::RuntimeConfig;
use appcore_contracts::{ApplicationManifestV1, DeploymentManifestV1, LeadershipMode, RuntimeMode};
use appcore_core::AppPlugin;

pub(crate) fn application_manifest(
    config: &RuntimeConfig,
    plugin: &dyn AppPlugin,
) -> Result<ApplicationManifestV1, BootstrapError> {
    let manifest = plugin.application_manifest();
    manifest.validate().map_err(|error| {
        manifest_error(format!("application manifest validation failed: {error}"))
    })?;
    if manifest.application_id().as_str() != config.app_id {
        return Err(manifest_error(
            "application manifest identity does not match host configuration",
        ));
    }
    validate_runtime_requirements(
        &manifest,
        env!("CARGO_PKG_VERSION"),
        &config.protocol_version.to_string(),
    )?;
    Ok(manifest)
}

pub(crate) fn validate_runtime_requirements(
    manifest: &ApplicationManifestV1,
    runtime_version: &str,
    protocol_version: &str,
) -> Result<(), BootstrapError> {
    let requirements = manifest.runtime_requirements();
    if requirements.protocol_version() != protocol_version {
        return Err(manifest_error(format!(
            "application requires protocol {}, host provides {protocol_version}",
            requirements.protocol_version()
        )));
    }
    let current = semver::Version::parse(runtime_version)
        .map_err(|error| manifest_error(format!("invalid host runtime version: {error}")))?;
    let minimum =
        semver::Version::parse(requirements.minimum_runtime_version()).map_err(|error| {
            manifest_error(format!(
                "invalid application minimum runtime version: {error}"
            ))
        })?;
    if current < minimum {
        return Err(manifest_error(format!(
            "application requires runtime >= {minimum}, host provides {current}"
        )));
    }
    if let Some(maximum) = requirements.maximum_runtime_version() {
        let maximum = semver::Version::parse(maximum).map_err(|error| {
            manifest_error(format!(
                "invalid application maximum runtime version: {error}"
            ))
        })?;
        if current > maximum {
            return Err(manifest_error(format!(
                "application requires runtime <= {maximum}, host provides {current}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_manifest_compatibility(
    application: &ApplicationManifestV1,
    deployment: &DeploymentManifestV1,
) -> Result<(), BootstrapError> {
    if application.application_id() != deployment.application_id() {
        return Err(manifest_error(
            "application and deployment manifests have different application IDs",
        ));
    }
    if application.update_policy().is_automatic() {
        let update = deployment.update_provider().ok_or_else(|| {
            manifest_error("automatic application updates require a deployment update provider")
        })?;
        if update.settings().get("artifact_kind").map(String::as_str) != Some("executable") {
            return Err(manifest_error(
                "automatic updates require an executable artifact activation policy",
            ));
        }
    }
    match deployment.mode() {
        RuntimeMode::Standalone => {
            if application.job_policy().is_enabled() {
                return Err(manifest_error(
                    "standalone deployments cannot enable distributed jobs",
                ));
            }
            if application
                .leadership()
                .iter()
                .any(|requirement| requirement.mode() != LeadershipMode::Disabled)
            {
                return Err(manifest_error(
                    "standalone deployments cannot require distributed leadership",
                ));
            }
        }
        RuntimeMode::Cluster => {
            if deployment.control_plane().is_none() {
                return Err(manifest_error(
                    "cluster deployment requires a control-plane provider",
                ));
            }
        }
    }
    if application.job_policy().is_enabled() && deployment.job_provider().is_none() {
        return Err(manifest_error(
            "application jobs require a deployment job provider",
        ));
    }
    Ok(())
}
