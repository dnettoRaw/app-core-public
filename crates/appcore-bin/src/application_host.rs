// =============================================================================
//        #######
//     ###       ###     F: application_host.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 10:59:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 11:51:10 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Manifest-first facade that owns Runtime infrastructure composition.

use crate::application::Application;
use crate::application_context::DeploymentContext;
#[path = "application_host_contract.rs"]
mod application_host_contract;
use crate::application_plugin::ManifestApplicationPlugin;
use crate::application_tasks::RegisteredApplicationTask;
use crate::bootstrap::{now_ms, BootstrapError, BootstrapResult};
use crate::manifest_bootstrap::{bootstrap_manifest_input, load_manifest_input};
use appcore_api::{ApiMethod, ApiRequest, CommandRequest, QueryRequest, QueryResponse};
use appcore_contracts::{ApplicationManifestV1, DeploymentManifestV1, RuntimeManifestV1};
use appcore_core::{
    AppFamily, AppId, CommandResult, NodeId, RuntimeContext, RuntimeContractVersion,
    RuntimeController, RuntimeError, RuntimeIdentity, RuntimeLifecycleEvent, RuntimeLifecycleState,
    RuntimeResult, SyncGroup,
};
use application_host_contract::{
    build_query_router, build_task_registry, query_response, query_validation_error,
    validate_business_contract,
};
use std::path::Path;
use std::time::Duration;

const COMMAND_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// Running application assembled exclusively from manifests and business code.
pub struct ManifestApplicationHost {
    runtime: BootstrapResult,
    deployment_context: DeploymentContext,
    application_tasks: Vec<RegisteredApplicationTask>,
    #[cfg(feature = "ai-alpha")]
    ai: Option<std::sync::Arc<crate::application_ai::AppCoreAiComponent>>,
}

/// Result of a bounded Runtime service probe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct ApplicationServiceReport {
    /// Whether the HTTP listener was selected and started.
    pub http_started: bool,
    /// Whether the sync receiver was selected and started.
    pub sync_started: bool,
    /// Whether the peer RPC listener was selected and started.
    pub peer_rpc_started: bool,
    /// Whether a control-plane worker was selected and started.
    pub control_plane_started: bool,
    /// Whether the application scheduler was selected and started.
    pub scheduler_started: bool,
    /// Whether automatic application update polling was selected and started.
    pub update_started: bool,
    /// Whether the deployment-selected Gateway listener was started.
    pub gateway_started: bool,
    /// Gateway execution state observed before the bounded probe shut it down.
    pub gateway_state: Option<appcore_gateway::GatewayRuntimeState>,
    /// Actual non-secret Gateway listener address observed during the probe.
    pub gateway_bind_address: Option<std::net::SocketAddr>,
    /// Whether discovery returned a peer-directory snapshot.
    pub discovery_ready: bool,
    /// Whether service-scoped leadership was acquired.
    pub service_lease_active: bool,
    /// Whether the opt-in post-1.0 AI component was selected and started.
    #[cfg(feature = "ai-alpha")]
    pub ai_started: bool,
}

impl ManifestApplicationHost {
    /// Loads both manifests, composes Runtime infrastructure and starts the
    /// application lifecycle.
    pub fn load(
        application_manifest: impl AsRef<Path>,
        deployment_manifest: impl AsRef<Path>,
        business: &dyn Application,
    ) -> Result<Self, BootstrapError> {
        let input =
            load_manifest_input(application_manifest.as_ref(), deployment_manifest.as_ref())?;
        let deployment_path = input
            .config
            .deployment_manifest_path
            .as_deref()
            .ok_or_else(|| {
                BootstrapError::Runtime("deployment manifest path is absent".to_string())
            })?;
        let deployment_context =
            DeploymentContext::resolve(&input.deployment, Path::new(deployment_path))?;
        business
            .configure(&deployment_context)
            .map_err(runtime_error)?;
        let plugin =
            ManifestApplicationPlugin::new(input.application.clone(), &input.deployment, business)
                .map_err(runtime_error)?;
        let mut runtime = bootstrap_manifest_input(input, &plugin)?;
        runtime.app_query_router = Some(build_query_router(business)?);
        let application_tasks = build_task_registry(business)?;
        validate_business_contract(&runtime, &application_tasks)?;
        Ok(Self {
            runtime,
            deployment_context,
            application_tasks,
            #[cfg(feature = "ai-alpha")]
            ai: None,
        })
    }

    /// Attaches an explicitly configured post-1.0 AI component.
    ///
    /// V1 manifests do not select this alpha component. The caller must also
    /// declare and register any application-owned AI capability contract.
    #[cfg(feature = "ai-alpha")]
    #[must_use]
    pub fn with_ai(
        mut self,
        component: std::sync::Arc<crate::application_ai::AppCoreAiComponent>,
    ) -> Self {
        self.ai = Some(component);
        self
    }

    /// Returns the AI facade only when an alpha component was attached.
    #[cfg(feature = "ai-alpha")]
    #[must_use]
    pub fn ai(&self) -> Option<crate::application_ai::ApplicationAi> {
        self.ai.as_ref().map(|component| component.facade())
    }

    /// Returns the validated application-owned manifest.
    pub fn application_manifest(&self) -> &ApplicationManifestV1 {
        &self.runtime.application_manifest
    }

    /// Returns the validated installation-owned manifest.
    pub fn deployment_manifest(&self) -> &DeploymentManifestV1 {
        &self.runtime.deployment_manifest
    }

    /// Returns validated installation bindings supplied to the application.
    pub fn deployment_context(&self) -> &DeploymentContext {
        &self.deployment_context
    }

    /// Returns the current Runtime-owned manifest.
    pub fn runtime_manifest(&self) -> Result<RuntimeManifestV1, BootstrapError> {
        crate::manifests::current_runtime_manifest(&self.runtime)
    }

    /// Dispatches a command through the Runtime controller.
    pub fn dispatch_command(&self, request: CommandRequest) -> RuntimeResult<CommandResult> {
        self.runtime.capability_policy.authorize_runtime_command(
            &request.command_name,
            request.idempotency_key.as_deref(),
            now_ms(),
        )?;
        let controller = self.runtime.controller.lock().clone();
        let identity = controller.instance().identity().clone();
        let envelope = request.to_envelope(
            identity.app_id.clone(),
            identity.node_id.clone(),
            now_ms(),
            self.runtime.config.api_max_payload_bytes,
        )?;
        let context = HostedRuntimeContext::from_identity(&identity);
        controller.dispatch_command(&envelope, &context)
    }

    /// Dispatches a side-effect-free application query.
    pub fn dispatch_query(&self, request: QueryRequest) -> RuntimeResult<QueryResponse> {
        request
            .validate(self.runtime.config.api_max_payload_bytes)
            .map_err(query_validation_error)?;
        self.runtime
            .capability_policy
            .authorize_runtime_query(&request.query_name, now_ms())?;
        let router = self
            .runtime
            .app_query_router
            .as_ref()
            .ok_or(RuntimeError::MissingConfiguration {
                name: "app_query_router",
            })?
            .lock()
            .clone();
        let name = appcore_api::QueryName::new(request.query_name.clone())?;
        let payload = request.payload_bytes();
        let response = router.dispatch_query(
            &name,
            ApiRequest {
                method: ApiMethod::Query,
                path: request.query_name,
                payload,
            },
        )?;
        Ok(query_response(response))
    }

    /// Returns the number of audited command outcomes.
    pub fn audit_len(&self) -> usize {
        self.runtime.controller.lock().instance().audit_log().len()
    }

    /// Reports whether the application lifecycle is accepting work.
    pub fn is_running(&self) -> bool {
        matches!(
            self.runtime.controller.lock().lifecycle().current(),
            RuntimeLifecycleState::Running | RuntimeLifecycleState::Degraded
        )
    }

    /// Stops the application lifecycle without exposing controller internals.
    pub fn shutdown(&self) -> Result<(), BootstrapError> {
        self.shutdown_with_timeout(COMMAND_DRAIN_TIMEOUT)
    }

    /// Stops accepting commands and waits up to `timeout` for admitted work.
    pub fn shutdown_with_timeout(&self, timeout: Duration) -> Result<(), BootstrapError> {
        let controller = self.runtime.controller.lock().clone();
        if controller.lifecycle().current() == RuntimeLifecycleState::Stopped {
            return Ok(());
        }
        if controller.lifecycle().current() != RuntimeLifecycleState::ShuttingDown {
            controller
                .apply_lifecycle_event(RuntimeLifecycleEvent::ShutdownRequested)
                .map_err(runtime_error)?;
        }
        drain_commands(&controller, timeout)?;
        controller
            .apply_lifecycle_event(RuntimeLifecycleEvent::ShutdownCompleted)
            .map(|_| ())
            .map_err(runtime_error)
    }

    /// Runs Runtime-owned services until a shutdown signal is received.
    pub fn run(self) -> Result<(), BootstrapError> {
        if self
            .runtime
            .application_manifest
            .update_policy()
            .is_automatic()
            && !crate::application_supervisor::is_managed_child()
        {
            return Err(BootstrapError::Runtime(
                "automatic updates require the process supervisor; start the application with \
                 appcore_bin::run_application"
                    .to_string(),
            ));
        }
        #[cfg(feature = "ai-alpha")]
        {
            crate::server::run_application_bootstrapped_with_ai(
                self.runtime,
                self.application_tasks,
                self.ai.map(|component| component.managed_service()),
            )
        }
        #[cfg(not(feature = "ai-alpha"))]
        crate::server::run_application_bootstrapped(self.runtime, self.application_tasks)
    }

    /// Starts selected services, waits for readiness up to `timeout`, then
    /// performs a graceful shutdown and returns the observed service state.
    pub fn probe_services(
        self,
        timeout: Duration,
    ) -> Result<ApplicationServiceReport, BootstrapError> {
        #[cfg(feature = "ai-alpha")]
        {
            crate::server::probe_application_bootstrapped_with_ai(
                self.runtime,
                self.application_tasks,
                timeout,
                self.ai.map(|component| component.managed_service()),
            )
        }
        #[cfg(not(feature = "ai-alpha"))]
        crate::server::probe_application_bootstrapped(self.runtime, self.application_tasks, timeout)
    }
}

pub(crate) fn drain_commands(
    controller: &RuntimeController,
    timeout: Duration,
) -> Result<(), BootstrapError> {
    if controller.wait_for_inflight(timeout) {
        return Ok(());
    }
    Err(BootstrapError::Runtime(
        "command drain exceeded shutdown timeout".to_string(),
    ))
}

/// Loads manifests from standard paths and runs one business implementation.
///
/// `APPCORE_APPLICATION_MANIFEST` and `APPCORE_DEPLOYMENT_MANIFEST` override
/// the default `application.toml` and `deployment.toml` paths.
pub fn run_application(business: &dyn Application) -> Result<(), BootstrapError> {
    let application_manifest = std::env::var("APPCORE_APPLICATION_MANIFEST")
        .unwrap_or_else(|_| "application.toml".to_string());
    let deployment_manifest = std::env::var("APPCORE_DEPLOYMENT_MANIFEST")
        .unwrap_or_else(|_| "deployment.toml".to_string());
    if std::env::var_os("APPCORE_UPDATE_SMOKE_TEST").is_some() {
        let host =
            ManifestApplicationHost::load(&application_manifest, &deployment_manifest, business)?;
        return host.shutdown();
    }
    if crate::application_supervisor::is_required(
        Path::new(&application_manifest),
        Path::new(&deployment_manifest),
    )? {
        return crate::application_supervisor::run(
            Path::new(&application_manifest),
            Path::new(&deployment_manifest),
        );
    }
    ManifestApplicationHost::load(application_manifest, deployment_manifest, business)?.run()
}

fn runtime_error(error: RuntimeError) -> BootstrapError {
    BootstrapError::Runtime(format!("runtime application error: {error:?}"))
}

#[derive(Debug, Clone)]
struct HostedRuntimeContext {
    app_id: AppId,
    app_family: AppFamily,
    sync_group: SyncGroup,
    runtime_contract: RuntimeContractVersion,
    node_id: NodeId,
}

impl HostedRuntimeContext {
    fn from_identity(identity: &RuntimeIdentity) -> Self {
        Self {
            app_id: identity.app_id.clone(),
            app_family: identity.app_family.clone(),
            sync_group: identity.sync_group.clone(),
            runtime_contract: identity.runtime_contract,
            node_id: identity.node_id.clone(),
        }
    }
}

impl RuntimeContext for HostedRuntimeContext {
    fn app_id(&self) -> &AppId {
        &self.app_id
    }

    fn app_family(&self) -> &AppFamily {
        &self.app_family
    }

    fn sync_group(&self) -> &SyncGroup {
        &self.sync_group
    }

    fn runtime_contract(&self) -> RuntimeContractVersion {
        self.runtime_contract
    }

    fn node_id(&self) -> &NodeId {
        &self.node_id
    }
}

#[cfg(test)]
#[path = "application_host_tests.rs"]
mod tests;
