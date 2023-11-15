// =============================================================================
//        #######
//     ###       ###     F: builder_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:42:05 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::RuntimeBuilder;
use crate::bus::CommandBus;
use crate::command::CommandRegistry;
use crate::context::RuntimeContext;
use crate::decision::{DecisionEngine, DecisionNode, DecisionOutcome, DecisionRegistry};
use crate::envelope::{CommandEnvelope, EventEnvelope};
use crate::error::RuntimeResult;
use crate::event::EventRegistry;
use crate::handler::{CommandHandler, CommandResult};
use crate::ids::{
    AppFamily, AppId, CommandName, EventName, NodeId, RuntimeContractVersion, StateName, SyncGroup,
};
use crate::lifecycle::RuntimeLifecycleState;
use crate::plugin::AppPlugin;
use crate::state::StateRegistry;
use crate::RuntimeIdentity;
use appcore_contracts::{ApplicationId, ApplicationManifestV1, RuntimeRequirements, ServiceId};

fn application_manifest() -> ApplicationManifestV1 {
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

fn runtime_identity(node_id: NodeId) -> RuntimeIdentity {
    RuntimeIdentity {
        app_id: AppId::new("example-app").unwrap(),
        app_family: AppFamily::new("example-family").unwrap(),
        sync_group: SyncGroup::new("dev").unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id,
    }
}

macro_rules! plugin_contract {
    () => {
        fn application_manifest(&self) -> ApplicationManifestV1 {
            application_manifest()
        }

        fn identity(&self, node_id: NodeId) -> RuntimeIdentity {
            runtime_identity(node_id)
        }
    };
}

struct StaticDecision {
    name: &'static str,
    outcome: DecisionOutcome,
}

impl DecisionNode for StaticDecision {
    fn name(&self) -> &str {
        self.name
    }

    fn decide(
        &self,
        _command: &CommandEnvelope,
        _context: &dyn RuntimeContext,
    ) -> RuntimeResult<DecisionOutcome> {
        Ok(self.outcome.clone())
    }
}

struct ValidPlugin;

impl AppPlugin for ValidPlugin {
    plugin_contract!();

    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        registry.register(CommandName::new("runtime.start".to_string()).unwrap())
    }

    fn register_events(&self, registry: &mut EventRegistry) -> RuntimeResult<()> {
        registry.register(EventName::new("RuntimeStarted".to_string()).unwrap())
    }

    fn register_states(&self, registry: &mut StateRegistry) -> RuntimeResult<()> {
        registry.register(StateName::new("Running".to_string()).unwrap())
    }

    fn register_decisions(&self, registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        let decision = StaticDecision {
            name: "can_execute_command",
            outcome: DecisionOutcome::Allow,
        };
        registry.register(&decision)
    }
}

struct StaticContext {
    app_id: AppId,
    app_family: AppFamily,
    sync_group: SyncGroup,
    runtime_contract: RuntimeContractVersion,
    node_id: NodeId,
}

impl RuntimeContext for StaticContext {
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

struct StartHandler;

impl CommandHandler for StartHandler {
    fn command_name(&self) -> CommandName {
        CommandName::new("runtime.start".to_string()).unwrap()
    }

    fn handle(
        &self,
        _command: &CommandEnvelope,
        _context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        let event = EventEnvelope::new(
            EventName::new("RuntimeStarted".to_string()).unwrap(),
            "evt-1".to_string(),
            AppId::new("example-app".to_string()).unwrap(),
            NodeId::new("node-a".to_string()).unwrap(),
            0,
            vec![],
        )?;
        Ok(CommandResult::accepted(vec![event]))
    }
}

struct HandlerPlugin;

impl AppPlugin for HandlerPlugin {
    plugin_contract!();

    fn register_commands(&self, _registry: &mut CommandRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    fn register_events(&self, _registry: &mut EventRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    fn register_states(&self, _registry: &mut StateRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    fn register_decisions(&self, _registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        bus.register_handler(StartHandler)
    }
}

struct DenyDecisionPlugin;

impl AppPlugin for DenyDecisionPlugin {
    plugin_contract!();
    fn register_commands(&self, _registry: &mut CommandRegistry) -> RuntimeResult<()> {
        Ok(())
    }
    fn register_events(&self, _registry: &mut EventRegistry) -> RuntimeResult<()> {
        Ok(())
    }
    fn register_states(&self, _registry: &mut StateRegistry) -> RuntimeResult<()> {
        Ok(())
    }
    fn register_decisions(&self, _registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        Ok(())
    }
    fn register_decision_nodes(&self, engine: &mut DecisionEngine) -> RuntimeResult<()> {
        engine.register_node(StaticDecision {
            name: "deny_all",
            outcome: DecisionOutcome::Deny("denied by policy".to_string()),
        })
    }
    fn register_handlers(&self, bus: &mut CommandBus) -> RuntimeResult<()> {
        bus.register_handler(StartHandler)
    }
}

struct DuplicateCommandPlugin;

impl AppPlugin for DuplicateCommandPlugin {
    plugin_contract!();

    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        registry.register(CommandName::new("runtime.start".to_string()).unwrap())?;
        registry.register(CommandName::new("runtime.start".to_string()).unwrap())
    }

    fn register_events(&self, _registry: &mut EventRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    fn register_states(&self, _registry: &mut StateRegistry) -> RuntimeResult<()> {
        Ok(())
    }

    fn register_decisions(&self, _registry: &mut DecisionRegistry) -> RuntimeResult<()> {
        Ok(())
    }
}

#[test]
fn new_starts_empty() {
    let builder = RuntimeBuilder::new();

    assert!(builder.application_manifest().is_none());
    assert!(builder.commands().is_empty());
    assert!(builder.events().is_empty());
    assert!(builder.states().is_empty());
    assert!(builder.decisions().is_empty());
    assert!(builder.command_bus().is_empty());
    assert!(builder.event_bus.is_empty());
    assert!(builder.audit_log.is_empty());
}

#[test]
fn with_plugin_registers_manifest() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();

    let result = builder.with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap());

    assert!(result.is_ok());
    assert!(builder.application_manifest().is_some());
}

#[test]
fn with_plugin_registers_command_event_state_decision() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();

    let result = builder.with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap());

    assert!(result.is_ok());
    assert!(builder
        .commands()
        .contains(&CommandName::new("runtime.start".to_string()).unwrap()));
    assert!(builder
        .events()
        .contains(&EventName::new("RuntimeStarted".to_string()).unwrap()));
    assert!(builder
        .states()
        .contains(&StateName::new("Running".to_string()).unwrap()));
    assert!(builder.decisions().contains("can_execute_command"));
}

#[test]
fn with_plugin_rejects_second_plugin() {
    let first = ValidPlugin;
    let second = ValidPlugin;
    let mut builder = RuntimeBuilder::new();

    assert!(builder
        .with_plugin(&first, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    assert!(builder
        .with_plugin(&second, NodeId::new("node-b".to_string()).unwrap())
        .is_err());
}

#[test]
fn duplicate_registry_error_is_propagated() {
    let plugin = DuplicateCommandPlugin;
    let mut builder = RuntimeBuilder::new();

    let result = builder.with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap());

    assert!(result.is_err());
}

#[test]
fn build_without_plugin_fails() {
    let builder = RuntimeBuilder::new();
    let result = builder.build();
    assert!(result.is_err());
}

#[test]
fn build_with_plugin_creates_runtime_instance() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());

    let result = builder.build();
    assert!(result.is_ok());
}

#[test]
fn runtime_instance_exposes_manifest() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    assert_eq!(runtime.application_manifest().display_name(), "Example App");
}

#[test]
fn runtime_instance_exposes_lifecycle() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    assert_eq!(
        runtime.lifecycle().current(),
        RuntimeLifecycleState::Booting
    );
}

#[test]
fn runtime_instance_exposes_identity() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    assert_eq!(
        runtime.identity().app_id,
        AppId::new("example-app".to_string()).unwrap()
    );
}

#[test]
fn runtime_instance_exposes_registries() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    assert!(runtime
        .commands()
        .contains(&CommandName::new("runtime.start".to_string()).unwrap()));
    assert!(runtime
        .events()
        .contains(&EventName::new("RuntimeStarted".to_string()).unwrap()));
    assert!(runtime
        .states()
        .contains(&StateName::new("Running".to_string()).unwrap()));
    assert!(runtime.decisions().contains("can_execute_command"));
}

#[test]
fn runtime_instance_ensure_compatible_ok_for_compatible_identity() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    let other = plugin.identity(NodeId::new("node-b".to_string()).unwrap());
    assert!(runtime.ensure_compatible(&other).is_ok());
}

#[test]
fn runtime_instance_ensure_compatible_err_for_incompatible_identity() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    let mut other = plugin.identity(NodeId::new("node-b".to_string()).unwrap());
    other.app_id = AppId::new("other-app".to_string()).unwrap();

    assert!(runtime.ensure_compatible(&other).is_err());
}

#[test]
fn plugin_registers_handler_via_runtime_builder() {
    let plugin = HandlerPlugin;
    let mut builder = RuntimeBuilder::new();

    let result = builder.with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap());

    assert!(result.is_ok());
    assert!(builder
        .command_bus()
        .contains_handler(&CommandName::new("runtime.start".to_string()).unwrap()));
}

#[test]
fn plugin_registers_decision_node_in_builder() {
    let plugin = DenyDecisionPlugin;
    let mut builder = RuntimeBuilder::new();
    let result = builder.with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap());
    assert!(result.is_ok());

    let command = CommandEnvelope::new(
        CommandName::new("runtime.start".to_string()).unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        None,
        vec![],
    );
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let context = StaticContext {
        app_id: AppId::new("example-app".to_string()).unwrap(),
        app_family: AppFamily::new("example-family".to_string()).unwrap(),
        sync_group: SyncGroup::new("dev".to_string()).unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("node-a".to_string()).unwrap(),
    };
    let decision = builder.decision_engine.evaluate(&command, &context);
    assert!(decision.is_ok());
    assert_eq!(
        decision.ok(),
        Some(DecisionOutcome::Deny("denied by policy".to_string()))
    );
}

#[test]
fn decision_registry_and_engine_are_aligned_after_plugin_registration() {
    let plugin = DenyDecisionPlugin;
    let mut builder = RuntimeBuilder::new();
    let result = builder.with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap());
    assert!(result.is_ok());

    assert!(builder.decisions().contains("deny_all"));
    assert_eq!(builder.decisions().len(), 1);
    assert_eq!(builder.decision_engine.len(), 1);
    assert_eq!(
        builder.decision_engine.node_names(),
        &[String::from("deny_all")]
    );
}

#[test]
fn runtime_instance_exposes_command_bus() {
    let plugin = HandlerPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    assert!(runtime
        .command_bus()
        .contains_handler(&CommandName::new("runtime.start".to_string()).unwrap()));
}

#[test]
fn runtime_instance_exposes_event_bus_read_only() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    assert!(runtime.event_bus().is_empty());
}

#[test]
fn runtime_instance_exposes_audit_log_read_only() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };

    assert!(runtime.audit_log().is_empty());
}

#[test]
fn runtime_instance_dispatch_command_calls_handler() {
    let plugin = HandlerPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    let command = CommandEnvelope::new(
        CommandName::new("runtime.start".to_string()).unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        None,
        vec![],
    );
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let context = StaticContext {
        app_id: AppId::new("example-app".to_string()).unwrap(),
        app_family: AppFamily::new("example-family".to_string()).unwrap(),
        sync_group: SyncGroup::new("dev".to_string()).unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("node-a".to_string()).unwrap(),
    };

    let dispatch_result = runtime.dispatch_command(&command, &context);
    assert!(dispatch_result.is_ok());
    let dispatch_result = match dispatch_result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(dispatch_result.is_accepted());
}

#[test]
fn runtime_instance_dispatch_command_returns_error_when_handler_missing() {
    let plugin = ValidPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    let command = CommandEnvelope::new(
        CommandName::new("runtime.unknown".to_string()).unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        None,
        vec![],
    );
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let context = StaticContext {
        app_id: AppId::new("example-app".to_string()).unwrap(),
        app_family: AppFamily::new("example-family".to_string()).unwrap(),
        sync_group: SyncGroup::new("dev".to_string()).unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("node-a".to_string()).unwrap(),
    };

    let dispatch_result = runtime.dispatch_command(&command, &context);
    assert!(dispatch_result.is_err());
}

#[test]
fn runtime_instance_dispatch_blocks_denied_command() {
    let plugin = DenyDecisionPlugin;
    let mut builder = RuntimeBuilder::new();
    assert!(builder
        .with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())
        .is_ok());
    let runtime_result = builder.build();
    assert!(runtime_result.is_ok());
    let runtime = match runtime_result {
        Ok(runtime) => runtime,
        Err(_) => return,
    };
    let command = CommandEnvelope::new(
        CommandName::new("runtime.start".to_string()).unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        None,
        vec![],
    );
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let context = StaticContext {
        app_id: AppId::new("example-app".to_string()).unwrap(),
        app_family: AppFamily::new("example-family".to_string()).unwrap(),
        sync_group: SyncGroup::new("dev".to_string()).unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("node-a".to_string()).unwrap(),
    };

    let dispatch_result = runtime.dispatch_command(&command, &context);
    assert!(dispatch_result.is_ok());
    let dispatch_result = match dispatch_result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(!dispatch_result.is_accepted());
    assert_eq!(dispatch_result.message(), Some("denied by policy"));
}
