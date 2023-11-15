// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Minimal immutable runtime instance built from runtime builder contracts.

use crate::audit::AuditLog;
use crate::bus::CommandBus;
use crate::command::CommandRegistry;
use crate::context::RuntimeContext;
use crate::decision::{DecisionEngine, DecisionOutcome, DecisionRegistry};
use crate::envelope::CommandEnvelope;
use crate::error::RuntimeResult;
use crate::event::EventRegistry;
use crate::event_bus::EventBus;
use crate::handler::CommandResult;
use crate::identity::RuntimeIdentity;
use crate::lifecycle::RuntimeLifecycle;
use crate::state::StateRegistry;
use appcore_contracts::ApplicationManifestV1;

/// Built runtime instance with manifest and registries.
#[derive(Debug)]
pub struct RuntimeInstance {
    pub(crate) application_manifest: ApplicationManifestV1,
    pub(crate) identity: RuntimeIdentity,
    pub(crate) command_registry: CommandRegistry,
    pub(crate) event_registry: EventRegistry,
    pub(crate) state_registry: StateRegistry,
    pub(crate) decision_registry: DecisionRegistry,
    pub(crate) decision_engine: DecisionEngine,
    pub(crate) command_bus: CommandBus,
    pub(crate) event_bus: EventBus,
    pub(crate) audit_log: AuditLog,
    pub(crate) lifecycle: RuntimeLifecycle,
}

impl RuntimeInstance {
    /// Returns the application-owned V1 manifest.
    pub fn application_manifest(&self) -> &ApplicationManifestV1 {
        &self.application_manifest
    }

    /// Returns Runtime/application identity.
    pub fn identity(&self) -> &RuntimeIdentity {
        &self.identity
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

    /// Returns declared decision nodes.
    pub fn decisions(&self) -> &DecisionRegistry {
        &self.decision_registry
    }

    /// Returns the command bus.
    pub fn command_bus(&self) -> &CommandBus {
        &self.command_bus
    }

    /// Returns the process lifecycle.
    pub fn lifecycle(&self) -> &RuntimeLifecycle {
        &self.lifecycle
    }

    /// Returns recently emitted events.
    pub fn event_bus(&self) -> &EventBus {
        &self.event_bus
    }

    /// Returns the process-local audit log.
    pub fn audit_log(&self) -> &AuditLog {
        &self.audit_log
    }

    /// Evaluates policy and dispatches one command.
    pub fn dispatch_command(
        &self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        match self.decision_engine.evaluate(command, context)? {
            DecisionOutcome::Allow => self.command_bus.dispatch(command, context),
            DecisionOutcome::Deny(message) => Ok(CommandResult::rejected(message)),
            DecisionOutcome::Defer(message) => Ok(CommandResult::rejected(message)),
        }
    }

    /// Verifies compatibility with another Runtime identity.
    pub fn ensure_compatible(&self, other: &RuntimeIdentity) -> RuntimeResult<()> {
        self.identity().ensure_compatible(other)
    }
}
