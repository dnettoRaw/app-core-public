// =============================================================================
//        #######
//     ###       ###     F: assembly.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/22 15:41:18 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/26 10:16:57 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use super::*;

struct DeploymentBootstrap {
    config: RuntimeConfig,
    deployment_manifest: DeploymentManifestV1,
    gateway_config: Option<appcore_gateway::GatewayConfig>,
    observations: InMemoryObservationSink,
    metrics: Arc<InMemoryMetrics>,
    observation_file_sink: FileObservationSink,
}

struct SecretBootstrap {
    provider: HashTokenProvider,
    security_ok: bool,
    warning: Option<String>,
}

struct ManifestBootstrap {
    application_manifest: ApplicationManifestV1,
    runtime_manifest: RuntimeManifestV1,
    core_identity: CoreIdentity,
}

struct ProviderComponents {
    provider_plan: DeploymentProviderPlan,
    coordination_store: Option<SharedCoordinationStore>,
    update_provider: Option<SharedUpdateProvider>,
    update_store: FileArtifactStore,
    storage_provider: FileStorageProvider,
}

struct RuntimeComponents {
    core_manifest: DistributedCoreManifest,
    operation_mode: Arc<Mutex<RuntimeOperationalMode>>,
    controller: RuntimeController,
    health_check: BasicHealthCheck,
    heartbeat_source: StaticHeartbeatSource,
}

struct SyncComponents {
    replication_log: Option<Arc<Mutex<Box<dyn ReplicationLog + Send>>>>,
    replication_log_path: Option<PathBuf>,
    checkpoint_store: Option<Arc<FileSyncCheckpointStore>>,
    checkpoint_path: Option<PathBuf>,
}

/// Bootstraps the generic runtime without an application plugin.
pub fn bootstrap_runtime(path: Option<&str>) -> Result<BootstrapResult, BootstrapError> {
    bootstrap_runtime_with_plugin(path, None)
}

/// Bootstraps the runtime and composes an optional application plugin.
pub fn bootstrap_runtime_with_plugin(
    path: Option<&str>,
    app_plugin: Option<&dyn AppPlugin>,
) -> Result<BootstrapResult, BootstrapError> {
    let input = crate::manifest_bootstrap::load_manifest_input_for_deployment(path)?;
    if let Some(plugin) = app_plugin {
        let plugin_manifest = crate::manifests::application_manifest(&input.config, plugin)?;
        if plugin_manifest != input.application {
            return Err(BootstrapError::Runtime(
                "application code and application manifest disagree".to_string(),
            ));
        }
    }
    bootstrap_runtime_from_manifest(
        input.config,
        input.application,
        input.deployment,
        app_plugin,
    )
}

pub(crate) fn bootstrap_runtime_from_manifest(
    config: RuntimeConfig,
    application_manifest: ApplicationManifestV1,
    deployment_manifest: DeploymentManifestV1,
    app_plugin: Option<&dyn AppPlugin>,
) -> Result<BootstrapResult, BootstrapError> {
    config.validate()?;
    let gateway_config =
        crate::gateway_service::gateway_config_from_manifest(&deployment_manifest)?;
    let (observations, metrics, observation_file_sink) =
        super::observability::start_observations(&config)?;
    emit_ready(
        &observations,
        ObservationKind::Configuration,
        "runtime.deployment_manifest.ready",
    );
    bootstrap_prepared(
        DeploymentBootstrap {
            config,
            deployment_manifest,
            gateway_config,
            observations,
            metrics,
            observation_file_sink,
        },
        application_manifest,
        app_plugin,
    )
}

fn bootstrap_prepared(
    mut deployment: DeploymentBootstrap,
    application_manifest: ApplicationManifestV1,
    app_plugin: Option<&dyn AppPlugin>,
) -> Result<BootstrapResult, BootstrapError> {
    let secrets = prepare_secrets(
        &deployment.config,
        &deployment.deployment_manifest,
        &deployment.observations,
    )?;
    emit_ready(
        &deployment.observations,
        ObservationKind::Configuration,
        "runtime.application_manifest.ready",
    );
    finalize_runtime_config(&mut deployment.config, &application_manifest)?;
    emit_ready(
        &deployment.observations,
        ObservationKind::Configuration,
        "runtime.configuration.ready",
    );
    let manifests = prepare_manifests(
        &deployment.config,
        &deployment.deployment_manifest,
        application_manifest,
        deployment.gateway_config.is_some(),
    )?;
    emit_ready(
        &deployment.observations,
        ObservationKind::Configuration,
        "runtime.runtime_manifest.ready",
    );
    let providers = prepare_providers(
        &deployment.config,
        &deployment.deployment_manifest,
        &deployment.observations,
    )?;
    emit_lifecycle_ready(&deployment.observations, "runtime.providers.ready");
    let runtime = prepare_runtime_components(
        &deployment.config,
        &manifests.application_manifest,
        &manifests.core_identity,
        &providers.storage_provider,
        secrets.security_ok,
        deployment.gateway_config.as_ref(),
        app_plugin,
    )?;
    emit_lifecycle_ready(&deployment.observations, "runtime.execution.ready");
    let sync = prepare_sync_components(&deployment.config)?;
    emit_lifecycle_ready(&deployment.observations, "runtime.services.ready");
    let leader_lease = Arc::new(Mutex::new(None));
    let capability_policy = compose_capability_policy(&runtime, &manifests, &leader_lease)?;
    crate::gateway_service::authorize_gateway_if_configured(
        &capability_policy,
        deployment.gateway_config.as_ref(),
    )?;
    let runtime_manifest = finalize_runtime_manifest(
        manifests.runtime_manifest,
        &runtime.health_check,
        &providers.storage_provider,
        secrets.security_ok,
        deployment.config.operation_mode,
    )?;
    emit_lifecycle_ready(&deployment.observations, "runtime.bootstrap.ready");

    Ok(BootstrapResult {
        config: deployment.config,
        application_manifest: manifests.application_manifest,
        deployment_manifest: deployment.deployment_manifest,
        provider_plan: providers.provider_plan,
        coordination_store: providers.coordination_store,
        update_provider: providers.update_provider,
        update_store: providers.update_store,
        runtime_manifest,
        observations: deployment.observations,
        metrics: deployment.metrics,
        observation_file_sink: deployment.observation_file_sink,
        core_identity: manifests.core_identity,
        core_manifest: runtime.core_manifest,
        gateway_config: deployment.gateway_config,
        capability_policy,
        operation_mode: runtime.operation_mode,
        peer_directory: Arc::new(Mutex::new(None)),
        leader_lease,
        app_query_router: None,
        controller: Arc::new(Mutex::new(runtime.controller)),
        storage_provider: providers.storage_provider,
        health_check: runtime.health_check,
        heartbeat_source: runtime.heartbeat_source,
        replication_log: sync.replication_log,
        replication_log_path: sync.replication_log_path,
        checkpoint_store: sync.checkpoint_store,
        checkpoint_path: sync.checkpoint_path,
        security_provider: secrets.provider,
        security_ok: secrets.security_ok,
        security_warning: secrets.warning,
    })
}

fn emit_lifecycle_ready(observations: &InMemoryObservationSink, name: &'static str) {
    emit_ready(observations, ObservationKind::Lifecycle, name);
}

fn compose_capability_policy(
    runtime: &RuntimeComponents,
    manifests: &ManifestBootstrap,
    leader_lease: &Arc<Mutex<Option<ServiceLeaderLease>>>,
) -> Result<Arc<RuntimeCapabilityPolicy>, BootstrapError> {
    RuntimeCapabilityPolicy::from_manifest(
        &runtime.core_manifest,
        Arc::clone(&runtime.operation_mode),
        manifests.application_manifest.service_id().clone(),
        Arc::clone(leader_lease),
    )
    .map(Arc::new)
    .map_err(|error| BootstrapError::Runtime(format!("capability composition failed: {error:?}")))
}

fn prepare_secrets(
    config: &RuntimeConfig,
    deployment_manifest: &DeploymentManifestV1,
    observations: &InMemoryObservationSink,
) -> Result<SecretBootstrap, BootstrapError> {
    let secret_provider = crate::providers::secret_provider(deployment_manifest)?;
    let loaded = load_deployment_security_provider(config, &secret_provider)?;
    let security_ok = verify_security(config, &loaded.provider)?;
    emit_ready(
        observations,
        ObservationKind::Security,
        "runtime.secret_provider.ready",
    );
    Ok(SecretBootstrap {
        provider: loaded.provider,
        security_ok,
        warning: loaded.warning,
    })
}

fn finalize_runtime_config(
    config: &mut RuntimeConfig,
    manifest: &ApplicationManifestV1,
) -> Result<(), BootstrapError> {
    config.service_id = manifest.service_id().as_str().to_string();
    config.validate()?;
    Ok(())
}

fn prepare_manifests(
    config: &RuntimeConfig,
    deployment_manifest: &DeploymentManifestV1,
    application_manifest: ApplicationManifestV1,
    gateway_enabled: bool,
) -> Result<ManifestBootstrap, BootstrapError> {
    crate::manifests::validate_manifest_compatibility(&application_manifest, deployment_manifest)?;
    let core_identity = config.core_identity()?;
    let initial_health = RuntimeHealth::new(RuntimeHealthStatus::Degraded, now_ms())
        .with_detail("bootstrap_status", "initializing")
        .map_err(|error| BootstrapError::Runtime(format!("runtime health failed: {error}")))?;
    let runtime_manifest = crate::manifests::runtime_manifest(
        config,
        &core_identity,
        &application_manifest,
        deployment_manifest,
        initial_health,
        gateway_enabled,
    )?;
    Ok(ManifestBootstrap {
        application_manifest,
        runtime_manifest,
        core_identity,
    })
}

fn prepare_providers(
    config: &RuntimeConfig,
    deployment_manifest: &DeploymentManifestV1,
    observations: &InMemoryObservationSink,
) -> Result<ProviderComponents, BootstrapError> {
    let provider_plan = crate::providers::provider_plan(deployment_manifest)?;
    let coordination_store = crate::providers::coordination_store(&provider_plan)?;
    let update_provider = crate::providers::update_provider(&provider_plan)?;
    let storage_provider = prepare_storage(config)?;
    let update_store = FileArtifactStore::new(PathBuf::from(&config.storage_path).join("updates"));
    emit_ready(
        observations,
        ObservationKind::Storage,
        "runtime.storage.ready",
    );
    Ok(ProviderComponents {
        provider_plan,
        coordination_store,
        update_provider,
        update_store,
        storage_provider,
    })
}

fn prepare_runtime_components(
    config: &RuntimeConfig,
    application_manifest: &ApplicationManifestV1,
    core_identity: &CoreIdentity,
    storage_provider: &FileStorageProvider,
    security_ok: bool,
    gateway_config: Option<&appcore_gateway::GatewayConfig>,
    app_plugin: Option<&dyn AppPlugin>,
) -> Result<RuntimeComponents, BootstrapError> {
    let operation_mode = Arc::new(Mutex::new(config.operation_mode));
    let controller = build_controller(config, application_manifest, app_plugin)?;
    let core_manifest =
        build_core_manifest(config, core_identity, application_manifest, gateway_config)?;
    let health_check = build_health_check(
        storage_provider,
        security_ok,
        controller.lifecycle().current(),
    );
    let heartbeat_source = build_heartbeat_source(config)?;
    Ok(RuntimeComponents {
        core_manifest,
        operation_mode,
        controller,
        health_check,
        heartbeat_source,
    })
}

fn emit_ready(
    observations: &InMemoryObservationSink,
    kind: ObservationKind,
    message: &'static str,
) {
    observations.emit(ObservationEvent::new(
        kind,
        ObservationSeverity::Info,
        message,
        now_ms(),
    ));
}

fn build_heartbeat_source(config: &RuntimeConfig) -> Result<StaticHeartbeatSource, BootstrapError> {
    let node_id = NodeId::new(config.node_id.clone()).map_err(|error| {
        BootstrapError::Runtime(format!("invalid node_id '{}': {error:?}", config.node_id))
    })?;
    Ok(StaticHeartbeatSource::new(node_id, now_ms()))
}

fn finalize_runtime_manifest(
    runtime_manifest: RuntimeManifestV1,
    health_check: &BasicHealthCheck,
    storage_provider: &FileStorageProvider,
    security_ok: bool,
    operation_mode: RuntimeOperationalMode,
) -> Result<RuntimeManifestV1, BootstrapError> {
    let runtime_health = crate::manifests::runtime_health_from_parts(
        health_check.check().status,
        storage_provider.health().status,
        security_ok,
        now_ms(),
    )?;
    runtime_manifest
        .with_health(runtime_health)
        .map(|manifest| manifest.with_operational_mode(operation_mode))
        .map_err(|error| BootstrapError::Runtime(format!("runtime manifest failed: {error}")))
}

fn prepare_sync_components(config: &RuntimeConfig) -> Result<SyncComponents, BootstrapError> {
    if !config.sync_enabled {
        return Ok(SyncComponents {
            replication_log: None,
            replication_log_path: None,
            checkpoint_store: None,
            checkpoint_path: None,
        });
    }

    let file_log = FileReplicationLog::new(&config.storage_path, "sync-replication.log")
        .map_err(|_| BootstrapError::Runtime("failed to init sync replication log".to_string()))?;
    let replication_log_path = file_log.file_path().to_path_buf();
    let checkpoint_path = PathBuf::from(&config.storage_path).join("sync-checkpoints.txt");
    let checkpoint_store = FileSyncCheckpointStore::new(checkpoint_path.clone())
        .map_err(|_| BootstrapError::Runtime("failed to init sync checkpoint store".to_string()))?;

    Ok(SyncComponents {
        replication_log: Some(Arc::new(Mutex::new(
            Box::new(file_log) as Box<dyn ReplicationLog + Send>
        ))),
        replication_log_path: Some(replication_log_path),
        checkpoint_store: Some(Arc::new(checkpoint_store)),
        checkpoint_path: Some(checkpoint_path),
    })
}

fn build_core_manifest(
    config: &RuntimeConfig,
    identity: &CoreIdentity,
    application_manifest: &ApplicationManifestV1,
    gateway_config: Option<&appcore_gateway::GatewayConfig>,
) -> Result<DistributedCoreManifest, BootstrapError> {
    let mut manifest =
        DistributedCoreManifest::from_application_manifest(application_manifest, identity.clone())
            .map_err(|error| BootstrapError::Runtime(format!("core manifest failed: {error:?}")))?;
    for (name, requirements) in &config.capability_requirements {
        let Some(descriptor) = manifest
            .capabilities
            .iter_mut()
            .find(|descriptor| descriptor.name.as_str() == name)
        else {
            return Err(BootstrapError::Runtime(format!(
                "capability requirement references unknown capability: {name}"
            )));
        };
        descriptor.requirements = *requirements;
    }
    crate::gateway_service::compose_gateway_capability(&mut manifest, gateway_config)?;
    manifest.endpoints = crate::manifests::peer_endpoints(config);
    manifest.metadata.insert(
        "operation_mode".to_string(),
        config.operation_mode.as_str().to_string(),
    );
    Ok(manifest)
}

fn build_health_check(
    storage: &FileStorageProvider,
    security_ok: bool,
    lifecycle: RuntimeLifecycleState,
) -> BasicHealthCheck {
    let storage_status = storage.health().status;
    let (status, message) = if !security_ok {
        (
            HealthStatus::Restricted,
            Some("security bootstrap failed".to_string()),
        )
    } else if lifecycle != RuntimeLifecycleState::Running {
        (
            HealthStatus::Degraded,
            Some("runtime is not running".to_string()),
        )
    } else if storage_status == StorageStatus::Online {
        (HealthStatus::Healthy, None)
    } else {
        (
            HealthStatus::Degraded,
            Some("storage is not online".to_string()),
        )
    };
    BasicHealthCheck::new("runtime.bootstrap", HealthReport { status, message })
}
