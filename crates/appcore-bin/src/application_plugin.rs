// =============================================================================
//        #######
//     ###       ###     F: application_plugin.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/23 10:59:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

use crate::application::Application;
use appcore_contracts::{ApplicationManifestV1, DeploymentManifestV1};
use appcore_core::{
    AppFamily, AppId, AppPlugin, CommandBus, CommandRegistry, DecisionEngine, DecisionRegistry,
    EventRegistry, NodeId, RuntimeContractVersion, RuntimeIdentity, RuntimeResult, StateRegistry,
    SyncGroup,
};

pub(crate) struct ManifestApplicationPlugin<'a> {
    business: &'a dyn Application,
    manifest: ApplicationManifestV1,
    app_id: AppId,
    app_family: AppFamily,
    sync_group: SyncGroup,
    runtime_contract: RuntimeContractVersion,
}

impl<'a> ManifestApplicationPlugin<'a> {
    pub(crate) fn new(
        manifest: ApplicationManifestV1,
        deployment: &DeploymentManifestV1,
        business: &'a dyn Application,
    ) -> RuntimeResult<Self> {
        let app_id = AppId::new(manifest.application_id().as_str())?;
        let app_family = AppFamily::new(manifest.application_id().as_str())?;
        let sync_group = SyncGroup::new(deployment.installation_id().as_str())?;
        let protocol = manifest
            .runtime_requirements()
            .protocol_version()
            .parse::<u16>()
            .map_err(|_| appcore_core::RuntimeError::IncompatibleRuntimeContract)?;
        Ok(Self {
            business,
            manifest,
            app_id,
            app_family,
            sync_group,
            runtime_contract: RuntimeContractVersion::new(protocol),
        })
    }
}

impl AppPlugin for ManifestApplicationPlugin<'_> {
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
        self.business.register_commands(registry)
    }

    fn register_events(&self, registry: &mut EventRegistry) -> RuntimeResult<()> {
        self.business.register_events(registry)
    }

    fn register_states(&self, registry: &mut StateRegistry) -> RuntimeResult<()> {
        self.business.register_states(registry)
    }

    fn register_decisions(&self, registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        self.business.register_decisions(registry)
    }

    fn register_decision_nodes(&self, engine: &mut DecisionEngine) -> RuntimeResult<()> {
        self.business.register_decision_nodes(engine)
    }

    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        self.business.register_handlers(bus)
    }
}
