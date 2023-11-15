// =============================================================================
//        #######
//     ###       ###     F: handler_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{CommandHandler, CommandResult};
use crate::context::RuntimeContext;
use crate::envelope::{CommandEnvelope, EventEnvelope};
use crate::error::RuntimeResult;
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

struct AcceptHandler;

impl CommandHandler for AcceptHandler {
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

#[test]
fn command_result_accepted_works() {
    let event = EventEnvelope::new(
        EventName::new("RuntimeStarted".to_string()).unwrap(),
        "evt-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        vec![],
    );
    assert!(event.is_ok());

    let event = match event {
        Ok(event) => event,
        Err(_) => return,
    };
    let result = CommandResult::accepted(vec![event]);

    assert!(result.is_accepted());
    assert_eq!(result.events().len(), 1);
    assert_eq!(result.message(), None);
}

#[test]
fn command_result_rejected_works() {
    let result = CommandResult::rejected("forbidden");

    assert!(!result.is_accepted());
    assert!(result.events().is_empty());
    assert_eq!(result.message(), Some("forbidden"));
}

#[test]
fn command_result_exposes_events() {
    let event = EventEnvelope::new(
        EventName::new("RuntimeStarted".to_string()).unwrap(),
        "evt-1".to_string(),
        AppId::new("example-app".to_string()).unwrap(),
        NodeId::new("node-a".to_string()).unwrap(),
        0,
        vec![1, 2],
    );
    assert!(event.is_ok());
    let event = match event {
        Ok(event) => event,
        Err(_) => return,
    };

    let result = CommandResult::accepted(vec![event]);
    assert_eq!(result.events().len(), 1);
}

#[test]
fn command_result_exposes_message() {
    let result = CommandResult::rejected("not allowed");
    assert_eq!(result.message(), Some("not allowed"));
}

#[test]
fn command_handler_mock_returns_accepted() {
    let handler = AcceptHandler;
    let context = StaticContext {
        app_id: AppId::new("example-app".to_string()).unwrap(),
        app_family: AppFamily::new("example-family".to_string()).unwrap(),
        sync_group: SyncGroup::new("dev".to_string()).unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
        node_id: NodeId::new("node-a".to_string()).unwrap(),
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

    let result = handler.handle(&command, &context);
    assert!(result.is_ok());
    let result = match result {
        Ok(result) => result,
        Err(_) => return,
    };
    assert!(result.is_accepted());
}
