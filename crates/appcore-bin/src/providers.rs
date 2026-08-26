// =============================================================================
//        #######
//     ###       ###     F: providers.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 13:21:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 10:16:57 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Deployment-driven provider composition for the runtime host.

use crate::bootstrap::BootstrapError;
use appcore_contracts::{DeploymentManifestV1, ProviderConfig, SecretRef, StorageRequirements};
use appcore_control_plane::require_secure_remote_endpoint;
use appcore_provider::{
    DeploymentProviderPlan, ProviderError, ProviderRegistry, ProviderResult, ProviderRole,
    SharedCoordinationStore,
};
use appcore_provider_vercel_neon::{SharedControlPlaneProvider, VercelNeonControlPlaneFactory};
use appcore_storage::{
    file_storage_capability_descriptor_v1, StorageCapabilityCatalogV1,
    StorageCapabilityRequirementsV1, StorageCapabilityV1,
};
use appcore_update::{FileUpdateProviderFactory, SharedUpdateProvider};

#[path = "provider_factories.rs"]
mod provider_factories;
use provider_factories::{
    required_setting, FileControlPlaneFactory, FileCoordinationStoreFactory,
    HttpControlPlaneFactory, InMemoryControlPlaneFactory, LocalMeshControlPlaneFactory,
};
#[path = "provider_secrets.rs"]
mod provider_secrets;
pub(crate) use provider_secrets::DeploymentSecretResolver;

pub(crate) fn provider_plan(
    manifest: &DeploymentManifestV1,
) -> Result<DeploymentProviderPlan, BootstrapError> {
    let plan = DeploymentProviderPlan::from_manifest(manifest);
    ensure_provider(plan.storage().provider_id().as_str(), &["file"], "storage")?;
    ensure_provider(
        plan.peer_transport().as_str(),
        &["http", "https", "mesh-relay"],
        "peer transport",
    )?;
    ensure_provider(
        plan.command_transport().as_str(),
        &["http", "https"],
        "command transport",
    )?;
    if let Some(discovery) = plan.peer_discovery() {
        ensure_provider(
            discovery.provider_id().as_str(),
            &["control-plane"],
            "peer discovery",
        )?;
    }
    if let Some(control_plane) = plan.control_plane() {
        ensure_provider(
            control_plane.provider_id().as_str(),
            &[
                "http-control-plane",
                "in-memory",
                "file-control-plane",
                "local-mesh",
                "vercel-neon",
            ],
            "control plane",
        )?;
    }
    if let Some(secret_provider) = plan.secret_provider() {
        ensure_provider(
            secret_provider.provider_id().as_str(),
            &["env-file", "file-keyring-v1"],
            "secret",
        )?;
    }
    if let Some(coordination_store) = plan.coordination_store() {
        ensure_provider(
            coordination_store.provider_id().as_str(),
            &["file-coordination-v2"],
            "coordination store",
        )?;
    }
    if let Some(job_provider) = plan.job_provider() {
        return Err(BootstrapError::Runtime(format!(
            "deployment selected unavailable job provider: {}",
            job_provider.provider_id()
        )));
    }
    if let Some(update) = plan.update() {
        ensure_provider(
            update.provider_id().as_str(),
            &[appcore_update::FILE_UPDATE_PROVIDER_ID],
            "update",
        )?;
        crate::update_service::validate_update_authenticity_config(update)?;
    }
    validate_reference_stack(&plan)?;
    Ok(plan)
}

pub(crate) fn validate_storage_preflight(
    application: &StorageRequirements,
    selected: &ProviderConfig,
) -> Result<(), BootstrapError> {
    let mut requirements = StorageCapabilityRequirementsV1::from_provider_config(selected)
        .map_err(storage_capability_error)?;
    if application.is_shared() {
        requirements.include(StorageCapabilityV1::MultiHost);
    }
    let mut catalog = StorageCapabilityCatalogV1::new();
    catalog
        .register(file_storage_capability_descriptor_v1().map_err(storage_capability_error)?)
        .map_err(storage_capability_error)?;
    catalog
        .validate(selected.provider_id(), &requirements)
        .map_err(storage_capability_error)
}

pub(crate) fn validate_production_profile(
    manifest: &DeploymentManifestV1,
) -> Result<(), BootstrapError> {
    if manifest.mode() == appcore_contracts::RuntimeMode::Cluster
        && (!secure_peer_transport_selected(manifest)
            || manifest.network().command_transport().as_str() != "https")
    {
        return Err(BootstrapError::Runtime(
            "production cluster requires HTTPS or mesh-relay peer transport and HTTPS command transport"
                .to_string(),
        ));
    }
    if manifest
        .network()
        .listen_addresses()
        .iter()
        .any(|address| !is_loopback_address(address))
        && !manifest.network().tls().is_enabled()
    {
        return Err(BootstrapError::Runtime(
            "production non-loopback listener requires deployment TLS/mTLS".to_string(),
        ));
    }
    let secret_provider = manifest.secret_provider().ok_or_else(|| {
        BootstrapError::Runtime(
            "production profile requires an explicit secret provider".to_string(),
        )
    })?;
    if secret_provider.provider_id().as_str() != "file-keyring-v1" {
        return Err(BootstrapError::Runtime(
            "production reference profile requires file-keyring-v1 or an externally certified host adapter"
                .to_string(),
        ));
    }
    if manifest
        .secrets()
        .get("runtime_security")
        .map(SecretRef::as_str)
        != Some("provider:active")
    {
        return Err(BootstrapError::Runtime(
            "production keyring profile requires runtime_security=provider:active".to_string(),
        ));
    }
    if let Some(control_plane) = manifest.control_plane() {
        if let Some(endpoint) = control_plane.endpoint() {
            require_https_for_remote_endpoint(endpoint).map_err(provider_error)?;
        }
    }
    if let Some(update) = manifest.update_provider() {
        validate_production_update_policy(update)?;
    }
    Ok(())
}

fn secure_peer_transport_selected(manifest: &DeploymentManifestV1) -> bool {
    match manifest.network().peer_transport().as_str() {
        "https" => true,
        "mesh-relay" => manifest
            .adapters()
            .get("mesh-relay")
            .and_then(ProviderConfig::endpoint)
            .is_some_and(|endpoint| require_https_for_remote_endpoint(endpoint).is_ok()),
        _ => false,
    }
}

fn validate_production_update_policy(
    update: &appcore_contracts::ProviderConfig,
) -> Result<(), BootstrapError> {
    if update.settings().contains_key("trusted_local")
        || update
            .settings()
            .get("allow_unsigned_local_artifacts")
            .map(String::as_str)
            == Some("true")
    {
        return Err(BootstrapError::Runtime(
            "production update policy forbids unsigned local artifacts".to_string(),
        ));
    }
    let has_signing_key = update
        .settings()
        .keys()
        .any(|name| name.starts_with("signing_key."));
    let has_channels = update
        .settings()
        .get("allowed_channels")
        .is_some_and(|value| !value.trim().is_empty());
    let has_origins = update
        .settings()
        .get("allowed_origins")
        .is_some_and(|value| !value.trim().is_empty());
    if !has_signing_key || !has_channels || !has_origins {
        return Err(BootstrapError::Runtime(
            "production update policy requires signing_key.<id>, allowed_channels and allowed_origins"
                .to_string(),
        ));
    }
    Ok(())
}

fn is_loopback_address(address: &str) -> bool {
    address.starts_with("127.")
        || address.starts_with("[::1]")
        || address.starts_with("::1")
        || address.starts_with("localhost")
}

pub(crate) fn secret_provider(
    manifest: &DeploymentManifestV1,
) -> Result<DeploymentSecretResolver, BootstrapError> {
    DeploymentSecretResolver::from_manifest(manifest)
}

pub(crate) fn control_plane_client(
    plan: &DeploymentProviderPlan,
) -> Result<Option<SharedControlPlaneProvider>, BootstrapError> {
    let Some(config) = plan.control_plane() else {
        return Ok(None);
    };
    let secrets = DeploymentSecretResolver::from_plan(plan)?;
    let mut registry: ProviderRegistry<SharedControlPlaneProvider> = ProviderRegistry::new();
    registry
        .register(HttpControlPlaneFactory)
        .and_then(|_| registry.register(InMemoryControlPlaneFactory))
        .and_then(|_| registry.register(FileControlPlaneFactory))
        .and_then(|_| registry.register(LocalMeshControlPlaneFactory))
        .and_then(|_| registry.register(VercelNeonControlPlaneFactory))
        .map_err(provider_error)?;
    registry
        .create(ProviderRole::ControlPlane, config, plan.context(), &secrets)
        .map(Some)
        .map_err(provider_error)
}

pub(crate) fn coordination_store(
    plan: &DeploymentProviderPlan,
) -> Result<Option<SharedCoordinationStore>, BootstrapError> {
    let Some(config) = plan.coordination_store() else {
        return Ok(None);
    };
    let secrets = DeploymentSecretResolver::from_plan(plan)?;
    let mut registry: ProviderRegistry<SharedCoordinationStore> = ProviderRegistry::new();
    registry
        .register(FileCoordinationStoreFactory)
        .map_err(provider_error)?;
    let store = registry
        .create(
            ProviderRole::CoordinationStore,
            config,
            plan.context(),
            &secrets,
        )
        .map_err(provider_error)?;
    store.ensure_compatible().map_err(provider_error)?;
    Ok(Some(store))
}

pub(crate) fn update_provider(
    plan: &DeploymentProviderPlan,
) -> Result<Option<SharedUpdateProvider>, BootstrapError> {
    let Some(config) = plan.update() else {
        return Ok(None);
    };
    let secrets = DeploymentSecretResolver::from_plan(plan)?;
    let mut registry: ProviderRegistry<SharedUpdateProvider> = ProviderRegistry::new();
    registry
        .register(FileUpdateProviderFactory)
        .map_err(provider_error)?;
    registry
        .create(ProviderRole::Update, config, plan.context(), &secrets)
        .map(Some)
        .map_err(provider_error)
}

fn ensure_provider(selected: &str, supported: &[&str], role: &str) -> Result<(), BootstrapError> {
    if supported.contains(&selected) {
        return Ok(());
    }
    Err(BootstrapError::Runtime(format!(
        "deployment selected unavailable {role} provider: {selected}"
    )))
}

fn provider_error(error: ProviderError) -> BootstrapError {
    BootstrapError::Runtime(error.to_string())
}

fn storage_capability_error(error: appcore_storage::StorageCapabilityError) -> BootstrapError {
    BootstrapError::Runtime(format!("storage capability preflight failed: {error}"))
}

fn validate_reference_stack(plan: &DeploymentProviderPlan) -> Result<(), BootstrapError> {
    let Some(control) = plan.control_plane() else {
        return Ok(());
    };
    if !matches!(
        control.provider_id().as_str(),
        "file-control-plane" | "local-mesh"
    ) {
        return Ok(());
    }
    let coordination = plan.coordination_store().ok_or_else(|| {
        BootstrapError::Runtime("file-control-plane requires file-coordination-v2".to_string())
    })?;
    if coordination.provider_id().as_str() != "file-coordination-v2" {
        return Err(BootstrapError::Runtime(
            "file-control-plane requires file-coordination-v2".to_string(),
        ));
    }
    let control_path = required_setting(control, "path").map_err(provider_error)?;
    let coordination_path = required_setting(coordination, "path").map_err(provider_error)?;
    if control_path != coordination_path {
        return Err(BootstrapError::Runtime(
            "file control plane and coordination store must share the same path".to_string(),
        ));
    }
    Ok(())
}

fn require_https_for_remote_endpoint(endpoint: &str) -> ProviderResult<()> {
    require_secure_remote_endpoint(endpoint)
        .map_err(|error| ProviderError::InvalidConfiguration(error.to_string()))
}

#[cfg(test)]
#[path = "providers_tests.rs"]
mod tests;
