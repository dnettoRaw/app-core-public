// =============================================================================
//        #######
//     ###       ###     F: plugin_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::AppPlugin;
use crate::bus::CommandBus;
use crate::command::CommandRegistry;
use crate::decision::{DecisionEngine, DecisionRegistry};
use crate::event::EventRegistry;
use crate::identity::RuntimeIdentity;
use crate::ids::{AppFamily, AppId, NodeId, RuntimeContractVersion, SyncGroup};
use crate::state::StateRegistry;
use appcore_contracts::{ApplicationId, ApplicationManifestV1, RuntimeRequirements, ServiceId};

struct ExamplePlugin;

impl AppPlugin for ExamplePlugin {
    fn application_manifest(&self) -> ApplicationManifestV1 {
        ApplicationManifestV1::new(
            ApplicationId::new("example-app").unwrap(),
            "1.0.0",
            "Example App",
            "Example Vendor",
            ServiceId::new("example-service").unwrap(),
            RuntimeRequirements::new("1.0.0", "1").unwrap(),
        )
        .unwrap()
    }

    fn identity(&self, node_id: NodeId) -> RuntimeIdentity {
        RuntimeIdentity {
            app_id: AppId::new("example-app").unwrap(),
            app_family: AppFamily::new("example-family").unwrap(),
            sync_group: SyncGroup::new("dev").unwrap(),
            runtime_contract: RuntimeContractVersion::new(1),
            node_id,
        }
    }

    fn register_commands(
        &self,
        _registry: &mut CommandRegistry,
    ) -> crate::error::RuntimeResult<()> {
        Ok(())
    }

    fn register_events(&self, _registry: &mut EventRegistry) -> crate::error::RuntimeResult<()> {
        Ok(())
    }

    fn register_states(&self, _registry: &mut StateRegistry) -> crate::error::RuntimeResult<()> {
        Ok(())
    }

    fn register_decisions(
        &self,
        _registry: &mut DecisionRegistry,
    ) -> crate::error::RuntimeResult<()> {
        Ok(())
    }
}

#[test]
fn manifest_and_identity_are_explicit() {
    let plugin = ExamplePlugin;
    let manifest = plugin.application_manifest();
    let identity = plugin.identity(NodeId::new("node-a").unwrap());

    assert_eq!(manifest.application_id().as_str(), "example-app");
    assert_eq!(manifest.display_name(), "Example App");
    assert_eq!(identity.app_id, AppId::new("example-app").unwrap());
    assert_eq!(identity.node_id, NodeId::new("node-a").unwrap());
}

#[test]
fn default_register_handlers_does_not_break_existing_plugins() {
    let plugin = ExamplePlugin;
    let mut bus = CommandBus::new();
    let result = plugin.register_handlers(&mut bus);

    assert!(result.is_ok());
    assert!(bus.is_empty());
}

#[test]
fn default_register_decision_nodes_does_not_break_existing_plugins() {
    let plugin = ExamplePlugin;
    let mut engine = DecisionEngine::new();
    let result = plugin.register_decision_nodes(&mut engine);

    assert!(result.is_ok());
    assert!(engine.is_empty());
}
