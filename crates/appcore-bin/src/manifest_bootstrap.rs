// =============================================================================
//        #######
//     ###       ###     F: manifest_bootstrap.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 10:59:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::bootstrap::{BootstrapError, BootstrapResult};
use crate::runtime_config::{sanitize_distributed_default, RuntimeConfig};
use appcore_contracts::{ApplicationManifestV1, CapabilityMode, DeploymentManifestV1, RuntimeMode};
use appcore_core::{AppPlugin, CapabilityRequirements, RuntimeOperationalMode};
use std::collections::HashMap;
use std::fs;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};

pub(crate) struct ManifestBootstrapInput {
    pub(crate) application: ApplicationManifestV1,
    pub(crate) deployment: DeploymentManifestV1,
    pub(crate) config: RuntimeConfig,
}

pub(crate) fn load_manifest_input(
    application_path: &Path,
    deployment_path: &Path,
) -> Result<ManifestBootstrapInput, BootstrapError> {
    let application_path = canonical_manifest_path(application_path, "application")?;
    let deployment_path = canonical_manifest_path(deployment_path, "deployment")?;
    let application = parse_application_manifest(&application_path)?;
    let deployment = parse_deployment_manifest(&deployment_path)?;
    if application.application_id() != deployment.application_id() {
        return Err(BootstrapError::Runtime(
            "application and deployment manifests have different application IDs".to_string(),
        ));
    }
    let config = runtime_config(&application, &deployment, &deployment_path)?;
    Ok(ManifestBootstrapInput {
        application,
        deployment,
        config,
    })
}

pub(crate) fn bootstrap_manifest_input(
    input: ManifestBootstrapInput,
    plugin: &dyn AppPlugin,
) -> Result<BootstrapResult, BootstrapError> {
    crate::bootstrap::bootstrap_runtime_from_manifest(
        input.config,
        input.application,
        input.deployment,
        Some(plugin),
    )
}

pub(crate) fn load_manifest_input_for_deployment(
    deployment_path: Option<&str>,
) -> Result<ManifestBootstrapInput, BootstrapError> {
    let deployment_path = deployment_path
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("APPCORE_DEPLOYMENT_MANIFEST").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("deployment.toml"));
    reject_removed_configuration(&deployment_path)?;
    let application_path = std::env::var_os("APPCORE_APPLICATION_MANIFEST")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            deployment_path
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("application.toml")
        });
    load_manifest_input(&application_path, &deployment_path)
}

fn reject_removed_configuration(path: &Path) -> Result<(), BootstrapError> {
    if path.file_name().and_then(|name| name.to_str()) == Some("runtime.toml") {
        return Err(BootstrapError::Runtime(
            "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
        ));
    }
    if let Ok(contents) = fs::read_to_string(path) {
        if contents.contains("app_id") && !contents.contains("manifest_version") {
            return Err(BootstrapError::Runtime(
                "NO MORE SUPPORTED PLEASE UPDATE".to_string(),
            ));
        }
    }
    Ok(())
}

fn canonical_manifest_path(path: &Path, kind: &str) -> Result<PathBuf, BootstrapError> {
    fs::canonicalize(path).map_err(|error| {
        BootstrapError::Runtime(format!(
            "failed to resolve {kind} manifest '{}': {error}",
            path.display()
        ))
    })
}

fn parse_application_manifest(path: &Path) -> Result<ApplicationManifestV1, BootstrapError> {
    parse_manifest(path, "application")
}

fn parse_deployment_manifest(path: &Path) -> Result<DeploymentManifestV1, BootstrapError> {
    parse_manifest(path, "deployment")
}

fn parse_manifest<T>(path: &Path, kind: &str) -> Result<T, BootstrapError>
where
    T: serde::de::DeserializeOwned,
{
    let contents = fs::read_to_string(path).map_err(|error| {
        BootstrapError::Runtime(format!(
            "failed to read {kind} manifest '{}': {error}",
            path.display()
        ))
    })?;
    toml::from_str(&contents).map_err(|error| {
        BootstrapError::Runtime(format!(
            "invalid {kind} manifest '{}': {error}",
            path.display()
        ))
    })
}

fn runtime_config(
    application: &ApplicationManifestV1,
    deployment: &DeploymentManifestV1,
    deployment_path: &Path,
) -> Result<RuntimeConfig, BootstrapError> {
    let installation_id = sanitize_distributed_default(deployment.installation_id().as_str());
    let app_id = application.application_id().as_str().to_string();
    let protocol_version = application
        .runtime_requirements()
        .protocol_version()
        .parse::<u16>()
        .map_err(|_| BootstrapError::Runtime("protocol version must be a u16".to_string()))?;
    let (api_host, api_port, api_enabled) = network_listener(deployment)?;
    let deployment_directory = deployment_path.parent().unwrap_or_else(|| Path::new("."));
    let mut config = RuntimeConfig {
        app_id: app_id.clone(),
        app_family: app_id.clone(),
        application_vendor: application.vendor().to_string(),
        service_id: application.service_id().as_str().to_string(),
        sync_group: deployment.installation_id().as_str().to_string(),
        node_id: runtime_owned_id("node", &installation_id),
        tenant_id: sanitize_distributed_default(&app_id),
        cluster_id: installation_id.clone(),
        core_id: runtime_owned_id("core", &installation_id),
        instance_id: runtime_owned_id("instance", &installation_id),
        core_kind: "operational".to_string(),
        protocol_version,
        capabilities: capability_names(application),
        capability_requirements: capability_requirements(application),
        storage_path: deployment_directory.join("storage").display().to_string(),
        backup_path: deployment_directory.join("backups").display().to_string(),
        api_enabled,
        api_require_token: api_enabled,
        api_public_status: true,
        api_max_payload_bytes: 65_536,
        api_host,
        api_port,
        sync_enabled: deployment.mode() == RuntimeMode::Cluster,
        sync_require_token: deployment.mode() == RuntimeMode::Cluster,
        sync_role: "follower".to_string(),
        sync_bind_host: "127.0.0.1".to_string(),
        sync_bind_port: 39_201,
        sync_peers: Vec::new(),
        sync_dns_enabled: false,
        sync_dns_seeds: Vec::new(),
        sync_dns_default_port: 39_201,
        sync_push_every_ticks: 10,
        security_provider: "hashtoken".to_string(),
        security_secret_path: String::new(),
        security_secret_env: None,
        security_allow_expired_secret: false,
        token_issuer: format!("appcore-{installation_id}"),
        token_audience: app_id,
        token_ttl_ms: Some(60_000),
        idempotency_ttl_ms: 86_400_000,
        api_mdns_enabled: false,
        api_mdns_service_name: "appcore".to_string(),
        control_plane_enabled: false,
        control_plane_url: String::new(),
        control_plane_heartbeat_interval_ms: 30_000,
        control_plane_request_timeout_ms: 5_000,
        control_plane_require_token: false,
        control_plane_token_env: "APPCORE_CONTROL_PLANE_TOKEN".to_string(),
        peer_rpc_enabled: false,
        peer_rpc_bind_host: "127.0.0.1".to_string(),
        peer_rpc_bind_port: 39_301,
        operation_mode: RuntimeOperationalMode::ReadWrite,
        runtime_mode: deployment.mode(),
        deployment_manifest_path: Some(deployment_path.display().to_string()),
        only_one: false,
        kill_others: false,
        supervisor_watchdog_enabled: deployment.supervisor().watchdog().is_enabled(),
        supervisor_watchdog_check_interval_ms: deployment
            .supervisor()
            .watchdog()
            .check_interval_ms(),
        supervisor_watchdog_stall_timeout_ms: deployment.supervisor().watchdog().stall_timeout_ms(),
    };
    require_runtime_secret(deployment)?;
    crate::manifests::apply_deployment_manifest(&mut config, deployment)?;
    Ok(config)
}

fn network_listener(
    deployment: &DeploymentManifestV1,
) -> Result<(String, u16, bool), BootstrapError> {
    let Some(raw) = deployment.network().listen_addresses().first() else {
        return Ok(("127.0.0.1".to_string(), 0, false));
    };
    let address = raw.parse::<SocketAddr>().map_err(|error| {
        BootstrapError::Runtime(format!(
            "invalid deployment listen address '{raw}': {error}"
        ))
    })?;
    Ok((address.ip().to_string(), address.port(), true))
}

fn capability_names(application: &ApplicationManifestV1) -> Vec<String> {
    application
        .capabilities()
        .iter()
        .map(|capability| capability.id().as_str().to_string())
        .collect()
}

fn capability_requirements(
    application: &ApplicationManifestV1,
) -> HashMap<String, CapabilityRequirements> {
    application
        .capabilities()
        .iter()
        .filter(|capability| capability.mode() == CapabilityMode::Command)
        .map(|capability| {
            (
                capability.id().as_str().to_string(),
                CapabilityRequirements {
                    requires_leader: capability.requires_leader(),
                    read_only: false,
                    idempotency_required: capability.idempotency_required(),
                },
            )
        })
        .collect()
}

fn require_runtime_secret(deployment: &DeploymentManifestV1) -> Result<(), BootstrapError> {
    if deployment.secrets().contains_key("runtime_security") {
        return Ok(());
    }
    Err(BootstrapError::Runtime(
        "deployment manifest must reference runtime_security".to_string(),
    ))
}

fn runtime_owned_id(prefix: &str, value: &str) -> String {
    const MAX_DISTRIBUTED_ID_BYTES: usize = 80;
    let available = MAX_DISTRIBUTED_ID_BYTES.saturating_sub(prefix.len() + 1);
    let mut value = value.chars().take(available).collect::<String>();
    while value.ends_with('-') {
        let _ = value.pop();
    }
    format!("{prefix}-{value}")
}
