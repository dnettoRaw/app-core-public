// =============================================================================
//        #######
//     ###       ###     F: prepared.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/09/04 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/09/04 00:00:00 by dnettoRaw
//      ###########      S: 1.0.0-rc.1
// =============================================================================

//! Validated bridge from application-owned behavior to Runtime registries.
//!
//! Use this module after manifests and any deployment bindings have been
//! validated, but before a deployment starts providers or workers. The value
//! owns registries and callbacks, is intentionally not cloneable, and performs
//! no I/O. `appcore-sdk` implements the bridge over `appcore-core`; a host may
//! consume its parts without making Core depend on the SDK.

#[cfg(feature = "scheduler")]
use crate::ApplicationTaskRegistry;
use crate::{AppResult, Application};
#[cfg(feature = "api")]
use appcore_api::ApiRouter;
use appcore_contracts::{ApplicationManifestV1, DeploymentManifestV1};
use appcore_core::{
    AppFamily, AppId, AppPlugin, CommandBus, CommandRegistry, DecisionEngine, DecisionRegistry,
    EventRegistry, NodeId, RuntimeBuilder, RuntimeContractVersion, RuntimeIdentity,
    RuntimeInstance, RuntimeResult, StateRegistry, SyncGroup,
};

/// Application behavior prepared for an explicit deployment boundary.
///
/// This value owns only in-memory registries and callbacks. It does not own a
/// listener, provider, worker or process lifecycle. Deployments may inspect the
/// registrations and then connect the independently owned capability crates.
pub struct PreparedApplication {
    runtime: RuntimeInstance,
    #[cfg(feature = "api")]
    queries: ApiRouter,
    #[cfg(feature = "scheduler")]
    tasks: ApplicationTaskRegistry,
}

impl std::fmt::Debug for PreparedApplication {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PreparedApplication")
            .field("runtime", &self.runtime)
            .finish_non_exhaustive()
    }
}

impl PreparedApplication {
    pub(crate) fn new<A>(
        manifest: ApplicationManifestV1,
        deployment: &DeploymentManifestV1,
        application: &A,
        node_id: NodeId,
    ) -> AppResult<Self>
    where
        A: Application + ?Sized,
    {
        let adapter = ApplicationPlugin::new(manifest, deployment, application)?;
        let mut builder = RuntimeBuilder::new();
        builder.with_plugin(&adapter, node_id)?;
        let runtime = builder.build()?;

        #[cfg(feature = "api")]
        let queries = {
            let mut router = ApiRouter::new();
            application.register_queries(&mut router)?;
            router.freeze_queries();
            router
        };

        #[cfg(feature = "scheduler")]
        let tasks = {
            let mut registry = ApplicationTaskRegistry::new();
            application.register_tasks(&mut registry)?;
            registry
        };

        Ok(Self {
            runtime,
            #[cfg(feature = "api")]
            queries,
            #[cfg(feature = "scheduler")]
            tasks,
        })
    }

    /// Returns the immutable Core runtime and its registered behavior.
    pub fn runtime(&self) -> &RuntimeInstance {
        &self.runtime
    }

    /// Returns the frozen query router prepared for an API host.
    #[cfg(feature = "api")]
    pub fn queries(&self) -> &ApiRouter {
        &self.queries
    }

    /// Returns the bounded task definitions prepared for a scheduler integration.
    #[cfg(feature = "scheduler")]
    pub fn tasks(&self) -> &ApplicationTaskRegistry {
        &self.tasks
    }

    /// Transfers every prepared registry to a composition root.
    #[cfg(not(any(feature = "api", feature = "scheduler")))]
    pub fn into_parts(self) -> RuntimeInstance {
        self.runtime
    }

    /// Transfers every prepared registry to a composition root.
    #[cfg(all(feature = "api", not(feature = "scheduler")))]
    pub fn into_parts(self) -> (RuntimeInstance, ApiRouter) {
        (self.runtime, self.queries)
    }

    /// Transfers every prepared registry to a composition root.
    #[cfg(all(not(feature = "api"), feature = "scheduler"))]
    pub fn into_parts(self) -> (RuntimeInstance, ApplicationTaskRegistry) {
        (self.runtime, self.tasks)
    }

    /// Transfers every prepared registry to a composition root.
    #[cfg(all(feature = "api", feature = "scheduler"))]
    pub fn into_parts(self) -> (RuntimeInstance, ApiRouter, ApplicationTaskRegistry) {
        (self.runtime, self.queries, self.tasks)
    }
}

struct ApplicationPlugin<'a, A: Application + ?Sized> {
    application: &'a A,
    manifest: ApplicationManifestV1,
    app_id: AppId,
    app_family: AppFamily,
    sync_group: SyncGroup,
    runtime_contract: RuntimeContractVersion,
}

impl<'a, A: Application + ?Sized> ApplicationPlugin<'a, A> {
    fn new(
        manifest: ApplicationManifestV1,
        deployment: &DeploymentManifestV1,
        application: &'a A,
    ) -> RuntimeResult<Self> {
        let application_id = manifest.application_id().as_str();
        let protocol = manifest
            .runtime_requirements()
            .protocol_version()
            .parse::<u16>()
            .map_err(|_| appcore_core::RuntimeError::IncompatibleRuntimeContract)?;
        Ok(Self {
            application,
            app_id: AppId::new(application_id)?,
            app_family: AppFamily::new(application_id)?,
            sync_group: SyncGroup::new(deployment.installation_id().as_str())?,
            runtime_contract: RuntimeContractVersion::new(protocol),
            manifest,
        })
    }
}

impl<A: Application + ?Sized> AppPlugin for ApplicationPlugin<'_, A> {
    fn application_manifest(&self) -> ApplicationManifestV1 {
        self.manifest.clone()
    }

    fn identity(&self, node_id: NodeId) -> RuntimeIdentity {
        RuntimeIdentity {
            app_id: self.app_id.clone(),
            app_family: self.app_family.clone(),
            sync_group: self.sync_group.clone(),
            runtime_contract: self.runtime_contract,
            node_id,
        }
    }

    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        self.application.register_commands(registry)
    }

    fn register_events(&self, registry: &mut EventRegistry) -> RuntimeResult<()> {
        self.application.register_events(registry)
    }

    fn register_states(&self, registry: &mut StateRegistry) -> RuntimeResult<()> {
        self.application.register_states(registry)
    }

    fn register_decisions(&self, registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        self.application.register_decisions(registry)
    }

    fn register_decision_nodes(&self, engine: &mut DecisionEngine) -> RuntimeResult<()> {
        self.application.register_decision_nodes(engine)
    }

    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        self.application.register_handlers(bus)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::{CommandName, CommandRegistry};
    use crate::App;

    struct DuplicateCommands;

    impl Application for DuplicateCommands {
        fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
            registry.register(CommandName::new("duplicate.command")?)?;
            registry.register(CommandName::new("duplicate.command")?)
        }
    }

    #[test]
    fn registration_failure_is_preserved() {
        let app = App::new("duplicate-app").expect("application defaults");
        let error = app
            .prepare(
                &DuplicateCommands,
                NodeId::new("duplicate-node").expect("node identity"),
            )
            .expect_err("duplicate registration must fail");
        assert!(matches!(error, crate::AppError::Runtime(_)));
    }
}
