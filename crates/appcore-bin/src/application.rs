// =============================================================================
//        #######
//     ###       ###     F: application.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 10:59:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Public application-facing contracts for manifest-first hosting.

pub use crate::application_context::{
    DeploymentContext, DeploymentEnvironmentValue, ResolvedVolumeMount,
};
pub use crate::application_host::{
    run_application, ApplicationServiceReport, ManifestApplicationHost,
};
pub use crate::application_tasks::ApplicationTaskRegistry;
pub use appcore_api::{
    ApiMethod, ApiRequest, ApiResponse, ApiRouter, CommandRequest, QueryEndpoint, QueryName,
    QueryRequest, QueryResponse,
};
pub use appcore_contracts::{
    ApplicationManifestV1, DeploymentManifestV1, RuntimeHealthStatus, RuntimeManifestV1,
    RuntimeMode,
};
pub use appcore_core::{
    CommandBus, CommandEnvelope, CommandHandler, CommandName, CommandRegistry, CommandResult,
    DecisionEngine, DecisionRegistry, EventEnvelope, EventName, EventRegistry, RuntimeContext,
    RuntimeResult, StateRegistry,
};
pub use appcore_scheduler::{RetryPolicy, ScheduledTask, TaskContext, TaskResult, TaskSchedule};

/// Business behavior hosted by AppCore.
///
/// Identity, manifests, providers, lifecycle and transport wiring are owned by
/// the Runtime. Implementations register only application behavior.
pub trait Application: Send + Sync {
    /// Applies validated installation bindings before behavior registration.
    fn configure(&self, _deployment: &DeploymentContext) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers command names exposed by the application.
    fn register_commands(&self, _registry: &mut CommandRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers event names emitted by the application.
    fn register_events(&self, _registry: &mut EventRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers application state contracts.
    fn register_states(&self, _registry: &mut StateRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers decision names for introspection.
    fn register_decisions(&self, _registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers executable decision nodes.
    fn register_decision_nodes(&self, _engine: &mut DecisionEngine) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers executable command handlers.
    fn register_handlers(&self, _bus: &mut CommandBus) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers side-effect-free application query endpoints.
    fn register_queries(&self, _router: &mut ApiRouter) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers bounded background tasks declared by the application.
    ///
    /// The Runtime owns scheduling threads, concurrency and shutdown. The
    /// application owns only task definitions and business callbacks.
    fn register_tasks(&self, _registry: &mut ApplicationTaskRegistry) -> RuntimeResult<()> {
        Ok(())
    }
}
