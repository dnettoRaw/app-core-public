// =============================================================================
//        #######
//     ###       ###     F: runtime_services.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 23:50:45 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Composes, supervises, reports, and shuts down Runtime-owned services.

use crate::application_host::ApplicationServiceReport;
use crate::bootstrap::{now_ms, BootstrapError, BootstrapResult};
use crate::control_plane_service::control_plane_service_if_enabled;
use crate::gateway_service::{gateway_service_if_enabled, GatewayServiceCandidate};
use crate::peer_rpc_service::peer_rpc_service_if_enabled;
use crate::scheduler_service::scheduler_service_if_enabled;
use crate::server::server_http::http_service_if_enabled;
use crate::server::RuntimeServer;
use crate::supervisor::{fetch_health_progress, SupervisorHealthProgress};
use crate::sync_cli::sync_service_if_enabled;
use crate::update_service::update_service_if_enabled;
use appcore_supervisor::{
    DependencyRequirement, ManagedResource, ManagedService, PassiveManagedService, RestartPolicy,
    ServiceActivationState, ServiceDescriptor, Supervisor, SupervisorDiagnosis,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

#[path = "runtime_services_monitor.rs"]
mod monitor;

use monitor::{join_monitor, start_supervisor_monitor, start_watchdog_monitor};

pub(crate) const SCHEDULER_SERVICE: &str = "scheduler";
pub(crate) const RUNTIME_SERVICE: &str = "runtime";
pub(crate) const SECURITY_SERVICE: &str = "security";
pub(crate) const OBSERVATION_SERVICE: &str = "observation";
pub(crate) const METRICS_SERVICE: &str = "metrics";
pub(crate) const CONTROL_PLANE_SERVICE: &str = "control-plane";
pub(crate) const JOBS_SERVICE: &str = "jobs";
pub(crate) const HTTP_SERVICE: &str = "http";
pub(crate) const SYNC_SERVICE: &str = "sync";
pub(crate) const PEER_RPC_SERVICE: &str = "peer-rpc";
pub(crate) const UPDATE_SERVICE: &str = "update";
pub(crate) const AUTH_SERVER_SERVICE: &str = "auth-server";
pub(crate) const GATEWAY_SERVICE: &str = "gateway";

struct SelectedServices {
    scheduler: bool,
    http: bool,
    sync: bool,
    peer_rpc: bool,
    control_plane: bool,
    update: bool,
    gateway: Option<Arc<appcore_gateway::GatewayRuntime>>,
}

struct ServiceCandidates {
    scheduler: Option<Arc<dyn ManagedService>>,
    http: Option<Arc<dyn ManagedService>>,
    sync: Option<Arc<dyn ManagedService>>,
    peer_rpc: Option<Arc<dyn ManagedService>>,
    control_plane: Option<Arc<dyn ManagedService>>,
    update: Option<Arc<dyn ManagedService>>,
    gateway: Option<GatewayServiceCandidate>,
}

pub(crate) struct RuntimeServices {
    shutdown: Arc<AtomicBool>,
    supervisor: Supervisor,
    supervisor_monitor: Option<JoinHandle<Result<(), BootstrapError>>>,
    watchdog_monitor: Option<JoinHandle<Result<(), BootstrapError>>>,
    selected: SelectedServices,
}

pub(crate) struct RuntimeDiagnosis {
    pub(crate) supervisor: SupervisorDiagnosis,
    pub(crate) external_health: Option<SupervisorHealthProgress>,
}

impl RuntimeServices {
    pub(crate) fn start(server: &mut RuntimeServer) -> Result<Self, BootstrapError> {
        let selected = register_runtime_services(server).inspect_err(|_| {
            let _ = server.request_shutdown();
        })?;
        if let Err(error) = server.service_supervisor.start_all() {
            return Err(fail_closed_startup(server, error));
        }
        let supervisor_monitor = match start_supervisor_monitor(
            server.service_supervisor.clone(),
            Arc::clone(&server.service_shutdown),
        ) {
            Ok(monitor) => monitor,
            Err(error) => return Err(fail_closed_after_start(server, error, None)),
        };
        let watchdog_monitor = match start_watchdog_monitor(
            server.service_supervisor.clone(),
            Arc::clone(&server.service_shutdown),
            Arc::clone(&server.app.operation_mode),
        ) {
            Ok(monitor) => monitor,
            Err(error) => {
                return Err(fail_closed_after_start(
                    server,
                    error,
                    Some(supervisor_monitor),
                ));
            }
        };
        Ok(Self {
            shutdown: Arc::clone(&server.service_shutdown),
            supervisor: server.service_supervisor.clone(),
            supervisor_monitor: Some(supervisor_monitor),
            watchdog_monitor: Some(watchdog_monitor),
            selected,
        })
    }

    pub(crate) fn diagnose(server: &mut RuntimeServer) -> Result<RuntimeDiagnosis, BootstrapError> {
        let _ = register_runtime_services(server).inspect_err(|_| {
            let _ = server.request_shutdown();
        })?;
        if let Err(error) = server.service_supervisor.start_all() {
            return Err(fail_closed_startup(server, error));
        }
        let diagnosis = diagnose_started_services(server);
        let shutdown = server
            .service_supervisor
            .shutdown(now_ms())
            .map_err(supervisor_error);
        let lifecycle = server.request_shutdown();
        let (diagnosis, external_health) = diagnosis?;
        shutdown?;
        lifecycle?;
        Ok(RuntimeDiagnosis {
            supervisor: diagnosis,
            external_health,
        })
    }

    pub(crate) fn report(&self, app: &BootstrapResult) -> ApplicationServiceReport {
        let gateway = self
            .selected
            .gateway
            .as_ref()
            .map(|runtime| runtime.snapshot());
        ApplicationServiceReport {
            http_started: self.selected.http,
            sync_started: self.selected.sync,
            peer_rpc_started: self.selected.peer_rpc,
            control_plane_started: self.selected.control_plane,
            scheduler_started: self.selected.scheduler,
            update_started: self.selected.update,
            gateway_started: gateway.as_ref().is_some_and(|snapshot| {
                snapshot.state == appcore_gateway::GatewayRuntimeState::Running
            }),
            gateway_state: gateway.as_ref().map(|snapshot| snapshot.state),
            gateway_bind_address: gateway.and_then(|snapshot| snapshot.bound_address),
            discovery_ready: app.peer_directory.lock().is_some(),
            service_lease_active: app.leader_lease.lock().is_some(),
        }
    }

    pub(crate) fn shutdown(mut self) -> Result<(), BootstrapError> {
        self.shutdown.store(true, Ordering::Release);
        let watchdog_result = join_monitor(self.watchdog_monitor.take(), "watchdog");
        let monitor_result = join_monitor(self.supervisor_monitor.take(), "supervisor");
        let shutdown_result = self.supervisor.shutdown(now_ms()).map_err(supervisor_error);
        watchdog_result.and(monitor_result).and(shutdown_result)
    }
}

fn runtime_health_url(config: &crate::runtime_config::RuntimeConfig) -> Option<String> {
    if !config.api_enabled {
        return None;
    }
    let host = match config.api_host.as_str() {
        "0.0.0.0" => "127.0.0.1".to_string(),
        "::" => "[::1]".to_string(),
        value if value.contains(':') && !value.starts_with('[') => format!("[{value}]"),
        value => value.to_string(),
    };
    Some(format!("http://{host}:{}/v1/health", config.api_port))
}

fn register_runtime_services(server: &RuntimeServer) -> Result<SelectedServices, BootstrapError> {
    let candidates = discover_services(server)?;
    let selected = SelectedServices {
        scheduler: candidates.scheduler.is_some(),
        http: candidates.http.is_some(),
        sync: candidates.sync.is_some(),
        peer_rpc: candidates.peer_rpc.is_some(),
        control_plane: candidates.control_plane.is_some(),
        update: candidates.update.is_some(),
        gateway: candidates
            .gateway
            .as_ref()
            .map(|candidate| Arc::clone(&candidate.runtime)),
    };
    register_core_services(&server.service_supervisor)?;
    register_selected_services(&server.service_supervisor, candidates)?;
    Ok(selected)
}

fn discover_services(server: &RuntimeServer) -> Result<ServiceCandidates, BootstrapError> {
    Ok(ServiceCandidates {
        scheduler: scheduler_service_if_enabled(
            server.app.application_manifest.scheduler_requirements(),
            &server.application_tasks,
            server.app.observations.clone(),
        )?,
        http: http_service_if_enabled(server)?,
        sync: sync_service_if_enabled(server)?,
        peer_rpc: peer_rpc_service_if_enabled(server)?,
        control_plane: control_plane_service_if_enabled(server)?,
        update: update_service_if_enabled(server)?,
        gateway: gateway_service_if_enabled(server)?,
    })
}

fn register_core_services(supervisor: &Supervisor) -> Result<(), BootstrapError> {
    register_passive(
        supervisor,
        RUNTIME_SERVICE,
        ManagedResource::Runtime,
        &[(SCHEDULER_SERVICE, DependencyRequirement::Optional)],
        ServiceActivationState::Enabled,
        true,
    )?;
    register_passive(
        supervisor,
        SECURITY_SERVICE,
        ManagedResource::Security,
        &[(RUNTIME_SERVICE, DependencyRequirement::Healthy)],
        ServiceActivationState::Enabled,
        true,
    )?;
    register_passive(
        supervisor,
        OBSERVATION_SERVICE,
        ManagedResource::Observation,
        &[(RUNTIME_SERVICE, DependencyRequirement::Ready)],
        ServiceActivationState::Enabled,
        false,
    )?;
    register_passive(
        supervisor,
        METRICS_SERVICE,
        ManagedResource::Metrics,
        &[(OBSERVATION_SERVICE, DependencyRequirement::Optional)],
        ServiceActivationState::Enabled,
        false,
    )?;
    register_passive(
        supervisor,
        JOBS_SERVICE,
        ManagedResource::Jobs,
        &[
            (SCHEDULER_SERVICE, DependencyRequirement::DegradedAllowed),
            (CONTROL_PLANE_SERVICE, DependencyRequirement::Optional),
        ],
        ServiceActivationState::NotConfigured,
        false,
    )?;
    register_passive(
        supervisor,
        AUTH_SERVER_SERVICE,
        ManagedResource::AuthServer,
        &[(SECURITY_SERVICE, DependencyRequirement::Healthy)],
        ServiceActivationState::NotConfigured,
        false,
    )
}

fn register_selected_services(
    supervisor: &Supervisor,
    candidates: ServiceCandidates,
) -> Result<(), BootstrapError> {
    register_or_inactive(
        supervisor,
        candidates.scheduler,
        SCHEDULER_SERVICE,
        ManagedResource::Scheduler,
        &[],
        ServiceActivationState::Disabled,
    )?;
    register_or_inactive(
        supervisor,
        candidates.control_plane,
        CONTROL_PLANE_SERVICE,
        ManagedResource::ControlPlane,
        &[RUNTIME_SERVICE],
        ServiceActivationState::Disabled,
    )?;
    register_or_inactive(
        supervisor,
        candidates.http,
        HTTP_SERVICE,
        ManagedResource::Http,
        &[SECURITY_SERVICE],
        ServiceActivationState::Disabled,
    )?;
    register_or_inactive(
        supervisor,
        candidates.sync,
        SYNC_SERVICE,
        ManagedResource::Sync,
        &[SECURITY_SERVICE],
        ServiceActivationState::Disabled,
    )?;
    register_or_inactive(
        supervisor,
        candidates.peer_rpc,
        PEER_RPC_SERVICE,
        ManagedResource::PeerRpc,
        &[SECURITY_SERVICE],
        ServiceActivationState::Disabled,
    )?;
    register_or_inactive(
        supervisor,
        candidates.update,
        UPDATE_SERVICE,
        ManagedResource::Update,
        &[SECURITY_SERVICE],
        ServiceActivationState::NotConfigured,
    )?;
    register_or_inactive(
        supervisor,
        candidates.gateway.map(|candidate| candidate.service),
        GATEWAY_SERVICE,
        ManagedResource::Gateway,
        &[SECURITY_SERVICE],
        ServiceActivationState::NotConfigured,
    )?;
    Ok(())
}

fn fail_closed_startup(
    server: &mut RuntimeServer,
    error: appcore_supervisor::SupervisorError,
) -> BootstrapError {
    fail_closed_after_start(
        server,
        BootstrapError::Runtime(format!("managed service startup failed: {error}")),
        None,
    )
}

fn fail_closed_after_start(
    server: &mut RuntimeServer,
    error: BootstrapError,
    monitor: Option<JoinHandle<Result<(), BootstrapError>>>,
) -> BootstrapError {
    server.service_shutdown.store(true, Ordering::Release);
    let mut details = vec![error.to_string()];
    if let Err(monitor_error) = join_monitor(monitor, "supervisor") {
        details.push(format!("monitor rollback failed: {monitor_error}"));
    }
    if let Err(rollback) = server.service_supervisor.shutdown(now_ms()) {
        details.push(format!("service rollback failed: {rollback}"));
    }
    if let Err(lifecycle) = server.request_shutdown() {
        details.push(format!("runtime shutdown failed: {lifecycle}"));
    }
    BootstrapError::Runtime(details.join("; "))
}

fn diagnose_started_services(
    server: &mut RuntimeServer,
) -> Result<(SupervisorDiagnosis, Option<SupervisorHealthProgress>), BootstrapError> {
    server
        .service_supervisor
        .reconcile(now_ms())
        .map_err(supervisor_error)?;
    thread::sleep(Duration::from_millis(25));
    server
        .service_supervisor
        .reconcile(now_ms())
        .map_err(supervisor_error)?;
    thread::sleep(Duration::from_millis(25));
    let external_health = runtime_health_url(&server.app.config)
        .as_deref()
        .and_then(fetch_health_progress);
    Ok((server.service_supervisor.diagnose(), external_health))
}

fn register_or_inactive(
    supervisor: &Supervisor,
    service: Option<Arc<dyn ManagedService>>,
    name: &str,
    resource: ManagedResource,
    dependencies: &[&str],
    inactive: ServiceActivationState,
) -> Result<(), BootstrapError> {
    match service {
        Some(service) => supervisor.register(service).map_err(supervisor_error),
        None => {
            let dependencies = dependencies
                .iter()
                .map(|dependency| (*dependency, DependencyRequirement::Healthy))
                .collect::<Vec<_>>();
            register_passive(supervisor, name, resource, &dependencies, inactive, false)
        }
    }
}

fn register_passive(
    supervisor: &Supervisor,
    name: &str,
    resource: ManagedResource,
    dependencies: &[(&str, DependencyRequirement)],
    activation: ServiceActivationState,
    critical: bool,
) -> Result<(), BootstrapError> {
    let descriptor = descriptor_with_requirements(name, resource, dependencies)?
        .with_activation(activation)
        .with_critical(critical);
    supervisor
        .register(Arc::new(PassiveManagedService::new(descriptor)))
        .map_err(supervisor_error)
}

pub(crate) fn service_descriptor(
    name: &str,
    resource: ManagedResource,
    dependencies: &[&str],
) -> Result<ServiceDescriptor, BootstrapError> {
    let policy = RestartPolicy::bounded(5, Duration::from_secs(600))
        .map_err(supervisor_error)?
        .with_backoff(Duration::from_millis(100), Duration::from_millis(50))
        .with_shutdown_timeout(Duration::from_secs(10));
    let mut descriptor =
        ServiceDescriptor::new(name, resource, policy).map_err(supervisor_error)?;
    for dependency in dependencies {
        descriptor = descriptor
            .with_dependency_requirement(*dependency, DependencyRequirement::Healthy)
            .map_err(supervisor_error)?;
    }
    Ok(descriptor)
}

fn descriptor_with_requirements(
    name: &str,
    resource: ManagedResource,
    dependencies: &[(&str, DependencyRequirement)],
) -> Result<ServiceDescriptor, BootstrapError> {
    let policy = RestartPolicy::bounded(5, Duration::from_secs(600))
        .map_err(supervisor_error)?
        .with_backoff(Duration::from_millis(100), Duration::from_millis(50))
        .with_shutdown_timeout(Duration::from_secs(10));
    let mut descriptor =
        ServiceDescriptor::new(name, resource, policy).map_err(supervisor_error)?;
    for (dependency, requirement) in dependencies {
        descriptor = descriptor
            .with_dependency_requirement(*dependency, *requirement)
            .map_err(supervisor_error)?;
    }
    Ok(descriptor)
}

fn supervisor_error(error: appcore_supervisor::SupervisorError) -> BootstrapError {
    BootstrapError::Runtime(format!("managed service supervision failed: {error}"))
}
