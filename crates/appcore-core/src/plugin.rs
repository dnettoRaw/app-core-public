// =============================================================================
//        #######
//     ###       ###     F: plugin.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Application plugin contract using concrete core registries.

use crate::bus::CommandBus;
use crate::command::CommandRegistry;
use crate::decision::{DecisionEngine, DecisionRegistry};
use crate::error::RuntimeResult;
use crate::event::EventRegistry;
use crate::identity::RuntimeIdentity;
use crate::ids::NodeId;
use crate::state::StateRegistry;
use appcore_contracts::ApplicationManifestV1;

/// Runtime-facing application plugin contract.
pub trait AppPlugin: Send + Sync {
    /// Returns the application-owned V1 contract.
    fn application_manifest(&self) -> ApplicationManifestV1;
    /// Returns the Runtime identity bound to the supplied node.
    fn identity(&self, node_id: NodeId) -> RuntimeIdentity;

    /// Declares command names implemented by the plugin.
    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()>;
    /// Declares event names emitted by the plugin.
    fn register_events(&self, registry: &mut EventRegistry) -> RuntimeResult<()>;
    /// Declares state names used by the plugin.
    fn register_states(&self, registry: &mut StateRegistry) -> RuntimeResult<()>;
    /// Registers decision names for catalog/manifest/introspection.
    fn register_decisions(&self, registry: &mut DecisionRegistry) -> RuntimeResult<()>;
    /// Registers executable decision nodes used at dispatch time.
    /// `RuntimeBuilder` keeps registry aligned with engine names after this step.
    fn register_decision_nodes(&self, _engine: &mut DecisionEngine) -> RuntimeResult<()> {
        Ok(())
    }

    /// Registers executable command handlers.
    fn register_handlers(&self, _bus: &mut CommandBus) -> RuntimeResult<()> {
        Ok(())
    }
}

#[cfg(test)]
#[path = "plugin_tests.rs"]
mod tests;
