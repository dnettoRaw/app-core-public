// =============================================================================
//        #######
//     ###       ###     F: builder.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Minimal runtime builder for wiring one plugin into core registries.

use crate::audit::AuditLog;
use crate::bus::CommandBus;
use crate::command::CommandRegistry;
use crate::decision::{DecisionEngine, DecisionRegistry};
use crate::error::{RuntimeError, RuntimeResult};
use crate::event::EventRegistry;
use crate::event_bus::EventBus;
use crate::identity::RuntimeIdentity;
use crate::ids::NodeId;
use crate::lifecycle::RuntimeLifecycle;
use crate::plugin::AppPlugin;
use crate::runtime::RuntimeInstance;
use crate::state::StateRegistry;
use appcore_contracts::ApplicationManifestV1;

/// Builder that aggregates one plugin manifest and public registries.
#[derive(Debug, Default)]
pub struct RuntimeBuilder {
    application_manifest: Option<ApplicationManifestV1>,
    identity: Option<RuntimeIdentity>,
    command_registry: CommandRegistry,
    event_registry: EventRegistry,
    state_registry: StateRegistry,
    decision_registry: DecisionRegistry,
    decision_engine: DecisionEngine,
    command_bus: CommandBus,
    event_bus: EventBus,
    audit_log: AuditLog,
    lifecycle: RuntimeLifecycle,
}

impl RuntimeBuilder {
    /// Creates an empty low-level Runtime composition builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the primary plugin and derives its node-scoped manifest.
    pub fn with_plugin<P: AppPlugin + ?Sized>(
        &mut self,
        plugin: &P,
        node_id: NodeId,
    ) -> RuntimeResult<&mut Self> {
        if self.application_manifest.is_some() {
            return Err(RuntimeError::PluginAlreadyRegistered);
        }

        let application_manifest = plugin.application_manifest();
        let identity = plugin.identity(node_id);
        plugin.register_commands(&mut self.command_registry)?;
        plugin.register_events(&mut self.event_registry)?;
        plugin.register_states(&mut self.state_registry)?;
        plugin.register_decisions(&mut self.decision_registry)?;
        plugin.register_decision_nodes(&mut self.decision_engine)?;
        self.sync_decision_registry_with_engine()?;
        plugin.register_handlers(&mut self.command_bus)?;
        self.application_manifest = Some(application_manifest);
        self.identity = Some(identity);

        Ok(self)
    }

    /// Adds behavior from another plugin without replacing identity.
    pub fn with_additional_plugin<P: AppPlugin + ?Sized>(
        &mut self,
        plugin: &P,
    ) -> RuntimeResult<&mut Self> {
        plugin.register_commands(&mut self.command_registry)?;
        plugin.register_events(&mut self.event_registry)?;
        plugin.register_states(&mut self.state_registry)?;
        plugin.register_decisions(&mut self.decision_registry)?;
        plugin.register_decision_nodes(&mut self.decision_engine)?;
        self.sync_decision_registry_with_engine()?;
        plugin.register_handlers(&mut self.command_bus)?;
        Ok(self)
    }

    fn sync_decision_registry_with_engine(&mut self) -> RuntimeResult<()> {
        for name in self.decision_engine.node_names() {
            if !self.decision_registry.contains(name) {
                self.decision_registry.register_name(name)?;
            }
        }
        Ok(())
    }

    /// Returns the primary application manifest, when configured.
    pub fn application_manifest(&self) -> Option<&ApplicationManifestV1> {
        self.application_manifest.as_ref()
    }

    /// Returns declared commands.
    pub fn commands(&self) -> &CommandRegistry {
        &self.command_registry
    }

    /// Returns declared events.
    pub fn events(&self) -> &EventRegistry {
        &self.event_registry
    }

    /// Returns declared states.
    pub fn states(&self) -> &StateRegistry {
        &self.state_registry
    }

    /// Returns declared decision node names.
    pub fn decisions(&self) -> &DecisionRegistry {
        &self.decision_registry
    }

    /// Returns the configured command bus.
    pub fn command_bus(&self) -> &CommandBus {
        &self.command_bus
    }

    /// Produces an immutable Runtime instance.
    pub fn build(self) -> RuntimeResult<RuntimeInstance> {
        let application_manifest = self
            .application_manifest
            .ok_or(RuntimeError::MissingManifest)?;
        let identity = self.identity.ok_or(RuntimeError::MissingManifest)?;

        Ok(RuntimeInstance {
            application_manifest,
            identity,
            command_registry: self.command_registry,
            event_registry: self.event_registry,
            state_registry: self.state_registry,
            decision_registry: self.decision_registry,
            decision_engine: self.decision_engine,
            command_bus: self.command_bus,
            event_bus: self.event_bus,
            audit_log: self.audit_log,
            lifecycle: self.lifecycle,
        })
    }
}

#[cfg(test)]
mod builder_tests;
