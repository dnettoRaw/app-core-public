// =============================================================================
//        #######
//     ###       ###     F: bus_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::CommandBus;
use crate::context::RuntimeContext;
use crate::envelope::{CommandEnvelope, EventEnvelope};
use crate::error::{RuntimeError, RuntimeResult};
use crate::handler::{CommandHandler, CommandResult};
use crate::ids::{
    AppFamily, AppId, CommandName, EventName, NodeId, RuntimeContractVersion, SyncGroup,
};

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

struct ErrorHandler;

impl CommandHandler for ErrorHandler {
    fn command_name(&self) -> CommandName {
        CommandName::new("runtime.fail".to_string()).unwrap()
    }

    fn handle(
        &self,
        _command: &CommandEnvelope,
        _context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        Err(RuntimeError::CommandRejected)
    }
}

fn context() -> StaticContext {
    StaticContext {
        app_id: AppId::new("example-app".to_string()).unwrap(),
        app_family: AppFamily::new("example-family".to_string()).unwrap(),
        sync_group: SyncGroup::new("dev".to_string()).unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("node-a".to_string()).unwrap(),
    }
}

fn command(name: &str) -> RuntimeResult<CommandEnvelope> {
    CommandEnvelope::new(
        CommandName::new(name.to_string()).unwrap(),
        "cmd-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        None,
        vec![],
    )
}

#[test]
fn new_starts_empty() {
    let bus = CommandBus::new();
    assert!(bus.is_empty());
    assert_eq!(bus.len(), 0);
}

#[test]
fn registers_handler() {
    let mut bus = CommandBus::new();
    let result = bus.register_handler(StartHandler);
    assert!(result.is_ok());
    assert_eq!(bus.len(), 1);
}

#[test]
fn rejects_duplicate_handler() {
    let mut bus = CommandBus::new();
    assert!(bus.register_handler(StartHandler).is_ok());
    let result = bus.register_handler(StartHandler);
    assert!(result.is_err());
}

#[test]
fn contains_handler_works() {
    let mut bus = CommandBus::new();
    assert!(bus.register_handler(StartHandler).is_ok());
    assert!(bus.contains_handler(&CommandName::new("runtime.start".to_string()).unwrap()));
    assert!(!bus.contains_handler(&CommandName::new("runtime.unknown".to_string()).unwrap()));
}

#[test]
fn dispatch_calls_correct_handler() {
    let mut bus = CommandBus::new();
    assert!(bus.register_handler(StartHandler).is_ok());
    let command = command("runtime.start");
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = bus.dispatch(&command, &context());
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(result.is_accepted());
    assert_eq!(result.events().len(), 1);
}

#[test]
fn dispatch_returns_error_when_handler_not_found() {
    let bus = CommandBus::new();
    let command = command("runtime.missing");
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = bus.dispatch(&command, &context());
    assert!(result.is_err());
}

#[test]
fn dispatch_propagates_handler_error() {
    let mut bus = CommandBus::new();
    assert!(bus.register_handler(ErrorHandler).is_ok());
    let command = command("runtime.fail");
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = bus.dispatch(&command, &context());
    assert!(result.is_err());
}
