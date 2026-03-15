// =============================================================================
//        #######
//     ###       ###     F: manifests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Versioned application, deployment, and runtime manifest composition.

use crate::bootstrap::{BootstrapError, BootstrapResult};
use crate::build_info::current_build_info;
use crate::runtime_config::RuntimeConfig;
use appcore_contracts::{
    ApplicationManifestV1, BuildId, CapabilityId, CoreId as ContractCoreId, CoreProfile, CoreRole,
    DeploymentManifestV1, FeatureId, LeadershipMode, LeadershipRequirement,
    NodeId as ContractNodeId, ResourceProfile, RuntimeHealth, RuntimeHealthStatus,
    RuntimeManifestV1, SchedulingProfile, SecretRef, WorkloadClass,
};
use appcore_core::{CoreIdentity, PeerEndpoint};
use appcore_ops::{HealthCheck, HealthStatus};
use appcore_storage::{StorageProvider, StorageStatus};
use std::fs;
use std::path::Path;

mod application;

#[cfg(test)]
use appcore_contracts::ApplicationId;
#[cfg(test)]
use application::validate_runtime_requirements;
pub(crate) use application::{application_manifest, validate_manifest_compatibility};

pub(crate) fn peer_endpoints(config: &RuntimeConfig) -> Vec<PeerEndpoint> {
    let mut endpoints = Vec::new();
    if config.api_enabled {
        endpoints.push(PeerEndpoint {
            name: "runtime-api".to_string(),
            url: format!("http://{}:{}", config.api_host, config.api_port),
            protocol: "http".to_string(),
            metadata: std::collections::BTreeMap::new(),
        });
    }
    if config.peer_rpc_enabled {
        endpoints.push(PeerEndpoint {
            name: "peer-rpc".to_string(),
            url: format!(
                "http://{}:{}",
                config.peer_rpc_bind_host, config.peer_rpc_bind_port
            ),
            protocol: "appcore-peer-rpc".to_string(),
            metadata: std::collections::BTreeMap::from([(
                "transport".to_string(),
                "http".to_string(),
            )]),
        });
    }
    if config.sync_enabled && config.sync_role == "follower" {
        endpoints.push(PeerEndpoint {
            name: "sync".to_string(),
            url: format!("http://{}:{}", config.sync_bind_host, config.sync_bind_port),
            protocol: "appcore-sync-v1".to_string(),
            metadata: std::collections::BTreeMap::from([(
                "transport".to_string(),
                "http".to_string(),
            )]),
        });
    }
    endpoints
}

pub(crate) fn load_deployment_manifest_for_config(
    config: &RuntimeConfig,
) -> Result<DeploymentManifestV1, BootstrapError> {
    let path = config
        .deployment_manifest_path
        .as_deref()
        .ok_or_else(|| manifest_error("NO MORE SUPPORTED PLEASE UPDATE"))?;
    load_deployment_manifest(path)
}

pub(crate) fn apply_deployment_manifest(
    config: &mut RuntimeConfig,
    manifest: &DeploymentManifestV1,
) -> Result<(), BootstrapError> {
    if manifest.application_id().as_str() != config.app_id {
        return Err(manifest_error(
            "deployment application_id does not match runtime configuration",
        ));
    }
    config.runtime_mode = manifest.mode();
    let deployment_directory = config
        .deployment_manifest_path
        .as_deref()
        .and_then(|path| Path::new(path).parent())
        .map(Path::to_path_buf);
    if let Some(path) = manifest.paths().get("storage") {
        config.storage_path =
            resolve_deployment_path(deployment_directory.as_deref(), path, "storage")?;
    }
    if let Some(path) = manifest.paths().get("backup") {
        config.backup_path =
            resolve_deployment_path(deployment_directory.as_deref(), path, "backup")?;
    }
    match manifest.mode() {
        appcore_contracts::RuntimeMode::Standalone => {
            config.control_plane_enabled = false;
            config.control_plane_url.clear();
            config.sync_dns_enabled = false;
            config.sync_dns_seeds.clear();
        }
        appcore_contracts::RuntimeMode::Cluster => {
            let control_plane = manifest
                .control_plane()
                .ok_or_else(|| manifest_error("cluster deployment requires a control plane"))?;
            config.control_plane_enabled = true;
            config.control_plane_url = match control_plane.endpoint() {
                Some(endpoint) => endpoint.to_string(),
                None if control_plane.provider_id().as_str() == "in-memory" => {
                    "memory://control-plane".to_string()
                }
                None if control_plane.provider_id().as_str() == "file-control-plane" => {
                    let path = control_plane
                        .settings()
                        .get("path")
                        .ok_or_else(|| manifest_error("file control plane requires path"))?;
                    format!("file://{path}")
                }
                None => {
                    return Err(manifest_error(
                        "remote cluster control-plane provider requires an endpoint",
                    ))
                }
            };
        }
    }
    if let Some(secret) = manifest.secrets().get("runtime_security") {
        apply_secret_ref(config, secret, deployment_directory.as_deref())?;
    }
    config.validate().map_err(BootstrapError::Config)
}

pub(crate) fn runtime_manifest(
    config: &RuntimeConfig,
    identity: &CoreIdentity,
    application: &ApplicationManifestV1,
    deployment: &DeploymentManifestV1,
    health: RuntimeHealth,
    gateway_enabled: bool,
) -> Result<RuntimeManifestV1, BootstrapError> {
    let service_id = application.service_id().clone();
    let mut capabilities = application
        .capabilities()
        .iter()
        .map(|capability| capability.id().clone())
        .collect::<Vec<_>>();
    if gateway_enabled {
        capabilities.push(
            CapabilityId::new(appcore_gateway::GATEWAY_RUNTIME_CAPABILITY)
                .map_err(|error| manifest_error(format!("invalid Gateway capability: {error}")))?,
        );
    }
    let leadership = application
        .leadership()
        .iter()
        .find(|requirement| requirement.service_id() == &service_id)
        .cloned()
        .unwrap_or(
            LeadershipRequirement::new(service_id.clone(), LeadershipMode::Disabled, 0)
                .map_err(|error| manifest_error(format!("invalid leadership profile: {error}")))?,
        );
    let cpu_cores = std::thread::available_parallelism()
        .ok()
        .and_then(|count| u16::try_from(count.get()).ok());
    let max_concurrency = application
        .scheduler_requirements()
        .max_concurrency()
        .max(4);
    let scheduling = SchedulingProfile::new(100, 0, max_concurrency, WorkloadClass::General)
        .map_err(|error| manifest_error(format!("invalid scheduling profile: {error}")))?;
    let profile = CoreProfile::new(
        if config.core_kind == appcore_core::CoreKind::OPERATIONAL {
            CoreRole::GeneralPurpose
        } else {
            CoreRole::Custom(config.core_kind.clone())
        },
        service_id,
        capabilities.clone(),
        leadership,
        ResourceProfile::new(cpu_cores, None, 0),
        scheduling,
    )
    .map_err(|error| manifest_error(format!("invalid core profile: {error}")))?;
    let build = current_build_info();
    let mut manifest = RuntimeManifestV1::new(
        build.version,
        identity.protocol_version.as_u16().to_string(),
        BuildId::new(build.build_id)
            .map_err(|error| manifest_error(format!("invalid build id: {error}")))?,
        ContractNodeId::new(identity.runtime.node_id.as_str())
            .map_err(|error| manifest_error(format!("invalid node id: {error}")))?,
        ContractCoreId::new(identity.core_id.as_str())
            .map_err(|error| manifest_error(format!("invalid core id: {error}")))?,
        deployment.mode(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        deployment.storage().provider_id().clone(),
        health,
        profile,
    )
    .map_err(|error| manifest_error(format!("runtime manifest failed: {error}")))?
    .with_feature(
        FeatureId::new("local-first")
            .map_err(|error| manifest_error(format!("invalid runtime feature: {error}")))?,
    );
    if deployment.mode() == appcore_contracts::RuntimeMode::Cluster {
        manifest =
            manifest
                .with_feature(FeatureId::new("distributed").map_err(|error| {
                    manifest_error(format!("invalid runtime feature: {error}"))
                })?);
    }
    if gateway_enabled {
        manifest =
            manifest
                .with_feature(FeatureId::new("gateway").map_err(|error| {
                    manifest_error(format!("invalid Gateway feature: {error}"))
                })?);
    }
    for capability in capabilities {
        manifest = manifest
            .with_loaded_capability(capability)
            .map_err(|error| manifest_error(format!("loaded capability failed: {error}")))?;
    }
    Ok(manifest)
}

pub(crate) fn runtime_health(app: &BootstrapResult) -> Result<RuntimeHealth, BootstrapError> {
    let report = app.health_check.check();
    let storage = app.storage_provider.health();
    runtime_health_from_parts(
        report.status,
        storage.status,
        app.security_ok,
        crate::bootstrap::now_ms(),
    )
}

pub(crate) fn runtime_health_from_parts(
    health: HealthStatus,
    storage: StorageStatus,
    security_ok: bool,
    checked_at_ms: u64,
) -> Result<RuntimeHealth, BootstrapError> {
    let status = match health {
        HealthStatus::Healthy => RuntimeHealthStatus::Healthy,
        HealthStatus::Degraded => RuntimeHealthStatus::Degraded,
        HealthStatus::Restricted | HealthStatus::Stopped => RuntimeHealthStatus::Unhealthy,
    };
    RuntimeHealth::new(status, checked_at_ms)
        .with_detail("storage_status", format!("{storage:?}"))
        .and_then(|health| {
            health.with_detail(
                "security_status",
                if security_ok { "ok" } else { "restricted" },
            )
        })
        .map_err(|error| manifest_error(format!("runtime health failed: {error}")))
}

pub(crate) fn current_runtime_manifest(
    app: &BootstrapResult,
) -> Result<RuntimeManifestV1, BootstrapError> {
    app.runtime_manifest
        .clone()
        .with_health(runtime_health(app)?)
        .map(|manifest| manifest.with_operational_mode(*app.operation_mode.lock()))
        .map_err(|error| manifest_error(format!("runtime manifest refresh failed: {error}")))
}

fn load_deployment_manifest(path: &str) -> Result<DeploymentManifestV1, BootstrapError> {
    let contents = fs::read_to_string(path)
        .map_err(|_| manifest_error("deployment manifest could not be read"))?;
    let extension = Path::new(path)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default();
    let manifest = if extension.eq_ignore_ascii_case("json") {
        serde_json::from_str(&contents)
            .map_err(|error| manifest_error(format!("invalid deployment JSON: {error}")))?
    } else {
        toml::from_str(&contents)
            .map_err(|error| manifest_error(format!("invalid deployment TOML: {error}")))?
    };
    Ok(manifest)
}

fn apply_secret_ref(
    config: &mut RuntimeConfig,
    secret: &SecretRef,
    deployment_directory: Option<&Path>,
) -> Result<(), BootstrapError> {
    let Some((scheme, target)) = secret.as_str().split_once(':') else {
        return Err(manifest_error("invalid runtime security secret reference"));
    };
    match scheme {
        "env" => {
            config.security_secret_env = Some(target.to_string());
            config.security_secret_path.clear();
        }
        "file" => {
            config.security_secret_env = None;
            config.security_secret_path =
                resolve_deployment_path(deployment_directory, target, "runtime_security")?;
        }
        "provider" => {
            config.security_secret_env = None;
            config.security_secret_path = format!("provider:{target}");
        }
        _ => {
            return Err(manifest_error(
                "runtime security provider supports env:, file: and provider: references only",
            ));
        }
    }
    Ok(())
}

fn resolve_deployment_path(
    deployment_directory: Option<&Path>,
    path: &str,
    field: &'static str,
) -> Result<String, BootstrapError> {
    let resolved = match deployment_directory {
        Some(directory) => crate::runtime_config::resolve_runtime_path(directory, path, field)
            .map_err(BootstrapError::Config)?,
        None => Path::new(path).to_path_buf(),
    };
    Ok(resolved.to_string_lossy().into_owned())
}

fn manifest_error(message: impl Into<String>) -> BootstrapError {
    BootstrapError::Runtime(message.into())
}

#[cfg(test)]
#[path = "manifests_tests.rs"]
mod tests;
