// =============================================================================
//        #######
//     ###       ###     F: controller_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:42:05 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::RuntimeController;
use crate::audit::AuditOutcome;
use crate::command::CommandRegistry;
use crate::context::RuntimeContext;
use crate::decision::DecisionRegistry;
use crate::envelope::{CommandEnvelope, EventEnvelope};
use crate::error::RuntimeResult;
use crate::event::EventRegistry;
use crate::handler::{CommandHandler, CommandResult};
use crate::ids::{
    AppFamily, AppId, CommandName, CoreId, EventName, NodeId, RuntimeContractVersion, SyncGroup,
    TenantId,
};
use crate::lifecycle::{RuntimeLifecycleEvent, RuntimeLifecycleState};
use crate::plugin::AppPlugin;
use crate::state::StateRegistry;
use crate::{RuntimeBuilder, RuntimeIdentity, TraceContext};
use appcore_contracts::{ApplicationId, ApplicationManifestV1, RuntimeRequirements, ServiceId};
use std::time::{SystemTime, UNIX_EPOCH};

fn assert_send_sync<T: Send + Sync>() {}

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

struct ControllerPlugin;

impl AppPlugin for ControllerPlugin {
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

    fn register_handlers(&self, bus: &mut crate::CommandBus) -> RuntimeResult<()> {
        bus.register_handler(StartHandler)
    }
}

fn build_instance() -> RuntimeResult<crate::runtime::RuntimeInstance> {
    let plugin = ControllerPlugin;
    let mut builder = RuntimeBuilder::new();
    builder.with_plugin(&plugin, NodeId::new("node-a".to_string()).unwrap())?;
    builder.build()
}

#[test]
fn new_keeps_instance() {
    assert_send_sync::<RuntimeController>();
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let controller = RuntimeController::new(instance);
    assert_eq!(
        controller.instance().application_manifest().display_name(),
        "Example App"
    );
}

#[test]
fn exposes_lifecycle() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let controller = RuntimeController::new(instance);
    assert_eq!(
        controller.lifecycle().current(),
        RuntimeLifecycleState::Booting
    );
}

#[test]
fn apply_lifecycle_event_changes_state() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    let result = controller.apply_lifecycle_event(RuntimeLifecycleEvent::ConfigLoaded);
    assert!(result.is_ok());
    assert_eq!(
        controller.lifecycle().current(),
        RuntimeLifecycleState::CheckingSecurity
    );
}

#[test]
fn invalid_transition_returns_error() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    let result = controller.apply_lifecycle_event(RuntimeLifecycleEvent::ApiStarted);
    assert_eq!(result, Err(crate::RuntimeError::InvalidStateTransition));
    assert_eq!(
        controller.lifecycle().current(),
        RuntimeLifecycleState::Booting
    );
}

fn make_context() -> StaticContext {
    StaticContext {
        app_id: AppId::new("example-app".to_string()).unwrap(),
        app_family: AppFamily::new("example-family".to_string()).unwrap(),
        sync_group: SyncGroup::new("dev".to_string()).unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("node-a".to_string()).unwrap(),
    }
}

fn make_command() -> RuntimeResult<CommandEnvelope> {
    CommandEnvelope::new(
        CommandName::new("runtime.start".to_string()).unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        None,
        vec![],
    )
}

fn make_command_with_key(key: Option<&str>) -> RuntimeResult<CommandEnvelope> {
    CommandEnvelope::new(
        CommandName::new("runtime.start".to_string()).unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        key.map(|value| value.to_string()),
        vec![],
    )
}

fn move_to_running(controller: &mut RuntimeController) {
    let _ = controller.apply_lifecycle_event(RuntimeLifecycleEvent::ConfigLoaded);
    let _ = controller.apply_lifecycle_event(RuntimeLifecycleEvent::SecurityChecked);
    let _ = controller.apply_lifecycle_event(RuntimeLifecycleEvent::StorageOpened);
    let _ = controller.apply_lifecycle_event(RuntimeLifecycleEvent::ApiStarted);
}

#[test]
fn controller_blocks_dispatch_in_booting() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    let context = make_context();
    let command = make_command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };

    let result = controller.dispatch_command(&command, &context);
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(!result.is_accepted());
    assert_eq!(result.message(), Some("runtime is not ready"));
}

#[test]
fn controller_allows_dispatch_in_running() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);

    let context = make_context();
    let command = make_command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = controller.dispatch_command(&command, &context);
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(result.is_accepted());
    assert_eq!(controller.instance().event_bus().len(), 1);
    assert_eq!(controller.instance().audit_log().len(), 1);
}

#[test]
fn controller_propagates_trace_to_events_and_audit() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);

    let context = make_context();
    let trace = TraceContext::new(
        "trace-1",
        "span-1",
        CoreId::new("core-a").unwrap(),
        CoreId::new("core-a").unwrap(),
        TenantId::new("tenant-a").unwrap(),
    )
    .unwrap()
    .with_command_id("cmd-1")
    .unwrap();
    let command = make_command().unwrap().with_trace(trace.clone());
    let result = controller.dispatch_command(&command, &context);

    assert!(result.is_ok());
    let events = controller.instance().event_bus().events();
    let audit = controller.instance().audit_log().records();
    assert_eq!(
        events[0].trace.as_ref().map(|item| item.trace_id.as_str()),
        Some("trace-1")
    );
    assert_eq!(
        audit[0].trace.as_ref().map(|item| item.trace_id.as_str()),
        Some("trace-1")
    );
}

#[test]
fn controller_allows_dispatch_in_degraded() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);
    assert!(controller
        .apply_lifecycle_event(RuntimeLifecycleEvent::DegradedDetected)
        .is_ok());

    let context = make_context();
    let command = make_command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = controller.dispatch_command(&command, &context);
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(result.is_accepted());
    assert_eq!(controller.instance().event_bus().len(), 1);
    assert_eq!(controller.instance().audit_log().len(), 1);
}

#[test]
fn controller_blocks_dispatch_in_restricted() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);
    assert!(controller
        .apply_lifecycle_event(RuntimeLifecycleEvent::RestrictedDetected)
        .is_ok());

    let context = make_context();
    let command = make_command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = controller.dispatch_command(&command, &context);
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(!result.is_accepted());
    assert_eq!(result.message(), Some("runtime is restricted"));
    assert!(controller.instance().event_bus().is_empty());
    assert_eq!(controller.instance().audit_log().len(), 1);
    assert_eq!(
        controller.instance().audit_log().records()[0].outcome,
        AuditOutcome::Rejected
    );
}

#[test]
fn controller_blocks_dispatch_in_shutting_down() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);
    assert!(controller
        .apply_lifecycle_event(RuntimeLifecycleEvent::ShutdownRequested)
        .is_ok());

    let context = make_context();
    let command = make_command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = controller.dispatch_command(&command, &context);
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(!result.is_accepted());
    assert_eq!(result.message(), Some("runtime is not ready"));
    assert!(controller.instance().event_bus().is_empty());
    assert_eq!(controller.instance().audit_log().len(), 1);
}

#[test]
fn runtime_instance_still_dispatches_without_lifecycle_check() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let context = make_context();
    let command = make_command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };

    let result = instance.dispatch_command(&command, &context);
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(result.is_accepted());
}

#[test]
fn dispatch_rejected_does_not_emit_events() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);
    assert!(controller
        .apply_lifecycle_event(RuntimeLifecycleEvent::RestrictedDetected)
        .is_ok());

    let context = make_context();
    let command = make_command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = controller.dispatch_command(&command, &context);
    assert!(result.is_ok());
    assert!(controller.instance().event_bus().is_empty());
    assert_eq!(controller.instance().audit_log().len(), 1);
}

#[test]
fn runtime_instance_direct_dispatch_does_not_emit_to_event_bus() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let context = make_context();
    let command = make_command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };

    let result = instance.dispatch_command(&command, &context);
    assert!(result.is_ok());
    assert!(instance.event_bus().is_empty());
    assert!(instance.audit_log().is_empty());
}

#[test]
fn controller_audits_error_when_handler_missing() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);
    let context = make_context();
    let command = CommandEnvelope::new(
        CommandName::new("runtime.unknown".to_string()).unwrap(),
        "cmd-missing".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        10,
        None,
        vec![],
    );
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };

    let result = controller.dispatch_command(&command, &context);
    assert!(result.is_err());
    assert!(controller.instance().event_bus().is_empty());
    assert_eq!(controller.instance().audit_log().len(), 1);
    assert_eq!(
        controller.instance().audit_log().records()[0].outcome,
        AuditOutcome::Error
    );
    assert_eq!(controller.idempotency_len(), 0);
}

#[test]
fn handler_error_does_not_poison_idempotency_retries() {
    let instance = build_instance().unwrap();
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);
    let context = make_context();
    let command = CommandEnvelope::new(
        CommandName::new("runtime.unknown").unwrap(),
        "cmd-retryable-error".to_string(),
        AppId::new("example-app").unwrap(),
        NodeId::new("node-a").unwrap(),
        10,
        Some("error-retry-key".to_string()),
        Vec::new(),
    )
    .unwrap();

    assert!(controller.dispatch_command(&command, &context).is_err());
    assert_eq!(controller.idempotency_len(), 0);
    assert!(controller.dispatch_command(&command, &context).is_err());
    assert_eq!(controller.idempotency_len(), 0);
    assert_eq!(controller.instance().audit_log().len(), 2);
}

#[test]
fn lifecycle_gate_runs_before_idempotency_check() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);
    assert!(controller
        .apply_lifecycle_event(RuntimeLifecycleEvent::RestrictedDetected)
        .is_ok());
    let context = make_context();
    let command = CommandEnvelope {
        command_name: CommandName::new("runtime.start").unwrap(),
        command_id: "cmd-1".to_string(),
        app_id: AppId::new("example-app").unwrap(),
        node_id: NodeId::new("node-a").unwrap(),
        issued_at_ms: 0,
        idempotency_key: Some("../bad".to_string()),
        payload: vec![],
        trace: None,
    };

    let result = controller.dispatch_command(&command, &context);
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert_eq!(result.message(), Some("runtime is restricted"));
    assert_eq!(controller.idempotency_len(), 0);
}

#[test]
fn command_without_idempotency_key_executes_every_time() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);
    let context = make_context();

    let first = make_command_with_key(None);
    let second = make_command_with_key(None);
    assert!(first.is_ok());
    assert!(second.is_ok());
    let first = match first {
        Ok(command) => command,
        Err(_) => return,
    };
    let second = match second {
        Ok(command) => command,
        Err(_) => return,
    };

    assert!(controller.dispatch_command(&first, &context).is_ok());
    assert!(controller.dispatch_command(&second, &context).is_ok());
    assert_eq!(controller.instance().event_bus().len(), 2);
    assert_eq!(controller.idempotency_len(), 0);
}

#[test]
fn command_with_idempotency_key_executes_once_and_rejects_duplicate() {
    let instance = build_instance();
    assert!(instance.is_ok());
    let instance = match instance {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let mut controller = RuntimeController::new(instance);
    move_to_running(&mut controller);
    let context = make_context();

    let first = make_command_with_key(Some("k-1"));
    let second = make_command_with_key(Some("k-1"));
    assert!(first.is_ok());
    assert!(second.is_ok());
    let first = match first {
        Ok(command) => command,
        Err(_) => return,
    };
    let second = match second {
        Ok(command) => command,
        Err(_) => return,
    };

    let first_result = controller.dispatch_command(&first, &context);
    assert!(first_result.is_ok());
    let second_result = controller.dispatch_command(&second, &context);
    assert!(second_result.is_ok());
    let second_result = match second_result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(second_result.is_accepted());
    assert_eq!(second_result.message(), None);
    assert_eq!(controller.instance().event_bus().len(), 1);
    assert_eq!(controller.instance().audit_log().len(), 2);
    assert_eq!(
        controller.instance().audit_log().records()[1].outcome,
        AuditOutcome::Accepted
    );
    assert_eq!(controller.idempotency_len(), 1);
    assert!(controller.idempotency_contains("k-1").unwrap_or(false));
}

#[test]
fn controller_deduplicates_across_reloads_via_file_idempotency() {
    let file = temp_idempotency_file("reloads");
    let first_instance = match build_instance() {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let first_store = crate::FileIdempotencyStore::new(&file);
    assert!(first_store.is_ok());
    let mut first_controller = RuntimeController::with_idempotency_store(
        first_instance,
        Box::new(match first_store {
            Ok(store) => store,
            Err(_) => return,
        }),
    );
    move_to_running(&mut first_controller);
    let context = make_context();
    let first_cmd = match make_command_with_key(Some("k-1")) {
        Ok(cmd) => cmd,
        Err(_) => return,
    };
    let second_cmd = first_cmd.clone();

    let first_result = first_controller.dispatch_command(&first_cmd, &context);
    assert!(first_result.is_ok());
    drop(first_controller);

    let second_instance = match build_instance() {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let second_store = crate::FileIdempotencyStore::new(&file);
    assert!(second_store.is_ok());
    let mut second_controller = RuntimeController::with_idempotency_store(
        second_instance,
        Box::new(match second_store {
            Ok(store) => store,
            Err(_) => return,
        }),
    );
    move_to_running(&mut second_controller);
    let second_result = second_controller.dispatch_command(&second_cmd, &context);
    assert!(second_result.is_ok());
    let second_result = match second_result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(second_result.is_accepted());
    assert_eq!(second_result.message(), None);
    assert_eq!(second_controller.instance().event_bus().len(), 0);

    let _ = std::fs::remove_file(file);
}

#[test]
fn duplicate_idempotency_key_is_rejected_after_restart_simulated() {
    let file = temp_idempotency_file("restart");
    let context = make_context();
    let first_cmd = match make_command_with_key(Some("k-restart")) {
        Ok(command) => command,
        Err(_) => return,
    };
    let second_cmd = match make_command_with_key(Some("k-restart")) {
        Ok(command) => command,
        Err(_) => return,
    };

    let first_instance = match build_instance() {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let first_store = crate::FileIdempotencyStore::new(&file);
    assert!(first_store.is_ok());
    let mut first_controller = RuntimeController::with_idempotency_store(
        first_instance,
        Box::new(match first_store {
            Ok(store) => store,
            Err(_) => return,
        }),
    );
    move_to_running(&mut first_controller);
    let first_result = first_controller.dispatch_command(&first_cmd, &context);
    assert!(first_result.is_ok());
    assert_eq!(first_controller.instance().event_bus().len(), 1);
    drop(first_controller);

    let second_instance = match build_instance() {
        Ok(instance) => instance,
        Err(_) => return,
    };
    let second_store = crate::FileIdempotencyStore::new(&file);
    assert!(second_store.is_ok());
    let mut second_controller = RuntimeController::with_idempotency_store(
        second_instance,
        Box::new(match second_store {
            Ok(store) => store,
            Err(_) => return,
        }),
    );
    move_to_running(&mut second_controller);
    let second_result = second_controller.dispatch_command(&second_cmd, &context);
    assert!(second_result.is_ok());
    let second_result = match second_result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(second_result.is_accepted());
    assert_eq!(second_result.message(), None);
    assert_eq!(second_controller.instance().event_bus().len(), 0);

    let _ = std::fs::remove_file(file);
}

fn temp_idempotency_file(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("appcore-controller-idemp-{name}-{nanos}.txt"))
}
