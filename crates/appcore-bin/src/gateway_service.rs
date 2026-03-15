// =============================================================================
//        #######
//     ###       ###     F: gateway_service.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/20 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/20 00:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Deployment, capability-policy, and Supervisor wiring for appcore-gateway.

use crate::bootstrap::{now_ms, BootstrapError};
use crate::capability_policy::RuntimeCapabilityPolicy;
use crate::server::RuntimeServer;
use appcore_contracts::{DeploymentManifestV1, RuntimeMode};
use appcore_core::DistributedCoreManifest;
use appcore_gateway::{
    gateway_capability_descriptor, GatewayConfig, GatewayRuntime, GatewayRuntimeState,
    GATEWAY_ADAPTER_NAME,
};
use appcore_ops::{
    InMemoryMetrics, InMemoryObservationSink, ObservationEvent, ObservationKind,
    ObservationSeverity, ObservationSink,
};
use appcore_peer_rpc::{FilePeerNonceStore, PeerNonceStore};
use appcore_supervisor::{
    ManagedService, ServiceDescriptor, ServiceHealth, ServiceRuntimeState, SupervisorError,
    SupervisorResult,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

const GATEWAY_REPLAY_PATH: &str = "gateway_replay";

pub(crate) struct GatewayServiceCandidate {
    pub(crate) service: Arc<dyn ManagedService>,
    pub(crate) runtime: Arc<GatewayRuntime>,
}

pub(crate) fn gateway_config_from_manifest(
    manifest: &DeploymentManifestV1,
) -> Result<Option<GatewayConfig>, BootstrapError> {
    manifest
        .adapters()
        .get(GATEWAY_ADAPTER_NAME)
        .map(GatewayConfig::from_provider_config)
        .transpose()
        .map_err(gateway_error)
}

pub(crate) fn authorize_gateway_if_configured(
    policy: &RuntimeCapabilityPolicy,
    config: Option<&GatewayConfig>,
) -> Result<(), BootstrapError> {
    if config.is_none() {
        return Ok(());
    }
    let descriptor = gateway_capability_descriptor().map_err(gateway_error)?;
    policy
        .authorize(descriptor.name.as_str(), descriptor.mode, None, now_ms())
        .map_err(|error| BootstrapError::Runtime(format!("gateway capability denied: {error:?}")))
}

pub(crate) fn compose_gateway_capability(
    manifest: &mut DistributedCoreManifest,
    config: Option<&GatewayConfig>,
) -> Result<(), BootstrapError> {
    let Some(config) = config else {
        return Ok(());
    };
    manifest
        .capabilities
        .push(gateway_capability_descriptor().map_err(gateway_error)?);
    manifest.metadata.insert(
        "gateway_bind_address".to_string(),
        config.bind_address.to_string(),
    );
    manifest.metadata.insert(
        "gateway_domain_suffix".to_string(),
        config.domain_suffix.clone(),
    );
    Ok(())
}

pub(crate) fn gateway_service_if_enabled(
    server: &RuntimeServer,
) -> Result<Option<GatewayServiceCandidate>, BootstrapError> {
    let Some(config) = server.app.gateway_config.clone() else {
        return Ok(None);
    };
    authorize_gateway_if_configured(&server.app.capability_policy, Some(&config))?;
    let replay_store = gateway_replay_store(server)?;
    let runtime = Arc::new(
        GatewayRuntime::with_replay_store(
            config,
            server.app.security_provider.clone(),
            replay_store,
        )
        .map_err(gateway_error)?,
    );
    let service = managed_gateway_service(
        Arc::clone(&runtime),
        server.app.observations.clone(),
        Arc::clone(&server.app.metrics),
    )?;
    Ok(Some(GatewayServiceCandidate { service, runtime }))
}

fn gateway_replay_store(server: &RuntimeServer) -> Result<Arc<dyn PeerNonceStore>, BootstrapError> {
    let deployment_directory = server
        .app
        .config
        .deployment_manifest_path
        .as_deref()
        .and_then(|path| Path::new(path).parent())
        .unwrap_or_else(|| Path::new("."));
    let path = selected_gateway_replay_path(
        server.app.deployment_manifest.mode(),
        server
            .app
            .deployment_manifest
            .paths()
            .get(GATEWAY_REPLAY_PATH)
            .map(String::as_str),
        deployment_directory,
        &server.app.config.storage_path,
    )?;
    FilePeerNonceStore::open(path)
        .map(|store| Arc::new(store) as Arc<dyn PeerNonceStore>)
        .map_err(|error| {
            BootstrapError::Runtime(format!(
                "gateway replay store initialization failed: {error}"
            ))
        })
}

fn selected_gateway_replay_path(
    mode: RuntimeMode,
    configured: Option<&str>,
    deployment_directory: &Path,
    storage_path: &str,
) -> Result<PathBuf, BootstrapError> {
    if let Some(path) = configured {
        if mode == RuntimeMode::Cluster && !Path::new(path).is_absolute() {
            return Err(BootstrapError::Runtime(
                "cluster paths.gateway_replay must be an absolute shared-volume file".to_string(),
            ));
        }
        return crate::runtime_config::resolve_runtime_path(
            deployment_directory,
            path,
            GATEWAY_REPLAY_PATH,
        )
        .map_err(BootstrapError::Config);
    }
    if mode == RuntimeMode::Cluster {
        return Err(BootstrapError::Runtime(
            "cluster Gateway requires paths.gateway_replay on one shared writable volume"
                .to_string(),
        ));
    }
    Ok(PathBuf::from(storage_path).join("security/gateway-connection-jti.json"))
}

fn managed_gateway_service(
    runtime: Arc<GatewayRuntime>,
    observations: InMemoryObservationSink,
    metrics: Arc<InMemoryMetrics>,
) -> Result<Arc<dyn ManagedService>, BootstrapError> {
    let descriptor = crate::runtime_services::service_descriptor(
        crate::runtime_services::GATEWAY_SERVICE,
        appcore_supervisor::ManagedResource::Gateway,
        &[crate::runtime_services::SECURITY_SERVICE],
    )?;
    Ok(Arc::new(ManagedGatewayService {
        descriptor,
        runtime,
        observations,
        metrics,
    }))
}

struct ManagedGatewayService {
    descriptor: ServiceDescriptor,
    runtime: Arc<GatewayRuntime>,
    observations: InMemoryObservationSink,
    metrics: Arc<InMemoryMetrics>,
}

impl ManagedService for ManagedGatewayService {
    fn descriptor(&self) -> &ServiceDescriptor {
        &self.descriptor
    }

    fn start(&self) -> SupervisorResult<()> {
        match self.runtime.start() {
            Ok(()) => {
                let snapshot = self.runtime.snapshot();
                let bind = snapshot
                    .bound_address
                    .unwrap_or(snapshot.configured_bind_address);
                self.emit(
                    ObservationSeverity::Info,
                    "runtime.gateway.started",
                    Some(bind),
                );
                let _ = self.metrics.increment("appcore.gateway.starts");
                Ok(())
            }
            Err(error) => {
                self.emit(
                    ObservationSeverity::Error,
                    "runtime.gateway.start_failed",
                    None,
                );
                let _ = self.metrics.increment("appcore.gateway.start_failures");
                Err(service_failure(error))
            }
        }
    }

    fn stop(&self, timeout: Duration) -> SupervisorResult<()> {
        match self.runtime.stop(timeout) {
            Ok(()) => {
                self.emit(ObservationSeverity::Info, "runtime.gateway.stopped", None);
                let _ = self.metrics.increment("appcore.gateway.stops");
                Ok(())
            }
            Err(error) => {
                self.emit(
                    ObservationSeverity::Error,
                    "runtime.gateway.stop_failed",
                    None,
                );
                let _ = self.metrics.increment("appcore.gateway.stop_failures");
                Err(service_failure(error))
            }
        }
    }

    fn health(&self) -> ServiceHealth {
        gateway_health(self.runtime.snapshot().state)
    }

    fn runtime_state(&self) -> ServiceRuntimeState {
        match self.runtime.snapshot().state {
            GatewayRuntimeState::Stopped => ServiceRuntimeState::Stopped,
            GatewayRuntimeState::Starting => ServiceRuntimeState::Starting,
            GatewayRuntimeState::Running => ServiceRuntimeState::Running,
            GatewayRuntimeState::Stopping => ServiceRuntimeState::Stopping,
            GatewayRuntimeState::Failed => ServiceRuntimeState::Failed,
            GatewayRuntimeState::Orphaned => ServiceRuntimeState::Orphaned,
        }
    }
}

impl ManagedGatewayService {
    fn emit(
        &self,
        severity: ObservationSeverity,
        name: &'static str,
        bind: Option<std::net::SocketAddr>,
    ) {
        let mut event = ObservationEvent::new(ObservationKind::Lifecycle, severity, name, now_ms());
        if let Some(bind) = bind {
            event = event.with_attribute("bind_address", bind.to_string());
        }
        self.observations.emit(event);
    }
}

fn service_failure(error: appcore_gateway::GatewayError) -> SupervisorError {
    SupervisorError::ServiceFailure {
        service: crate::runtime_services::GATEWAY_SERVICE.to_string(),
        reason: error.to_string(),
    }
}

fn gateway_health(state: GatewayRuntimeState) -> ServiceHealth {
    match state {
        GatewayRuntimeState::Running => ServiceHealth::Healthy,
        GatewayRuntimeState::Starting => ServiceHealth::Starting,
        GatewayRuntimeState::Stopping => ServiceHealth::Stopping,
        GatewayRuntimeState::Failed | GatewayRuntimeState::Orphaned => ServiceHealth::Failed,
        GatewayRuntimeState::Stopped => ServiceHealth::Unknown,
    }
}

fn gateway_error(error: appcore_gateway::GatewayError) -> BootstrapError {
    BootstrapError::Runtime(format!("gateway configuration failed: {error}"))
}

#[cfg(test)]
#[path = "gateway_service_tests.rs"]
mod tests;
