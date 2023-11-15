// =============================================================================
//        #######
//     ###       ###     F: decision_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/06/09 08:44:50 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{DecisionEngine, DecisionNode, DecisionOutcome, DecisionRegistry};
use crate::context::RuntimeContext;
use crate::envelope::CommandEnvelope;
use crate::ids::{AppFamily, AppId, CommandName, NodeId, RuntimeContractVersion, SyncGroup};

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
    ) -> crate::error::RuntimeResult<DecisionOutcome> {
        Ok(self.outcome.clone())
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

fn command() -> crate::error::RuntimeResult<CommandEnvelope> {
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

#[test]
fn register_decision() {
    let mut registry = DecisionRegistry::new();
    let decision = StaticDecision {
        name: "can_execute_command",
        outcome: DecisionOutcome::Allow,
    };

    let result = registry.register(&decision);

    assert!(result.is_ok());
    assert!(registry.contains("can_execute_command"));
    assert_eq!(registry.list(), &[String::from("can_execute_command")]);
}

#[test]
fn decision_engine_empty_allows() {
    let engine = DecisionEngine::new();
    let command = command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = engine.evaluate(&command, &context());
    assert!(result.is_ok());
    assert_eq!(result.ok(), Some(DecisionOutcome::Allow));
}

#[test]
fn decision_engine_allow_allows() {
    let mut engine = DecisionEngine::new();
    assert!(engine
        .register_node(StaticDecision {
            name: "allow",
            outcome: DecisionOutcome::Allow,
        })
        .is_ok());
    let command = command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = engine.evaluate(&command, &context());
    assert!(result.is_ok());
    assert_eq!(result.ok(), Some(DecisionOutcome::Allow));
}

#[test]
fn decision_engine_deny_blocks() {
    let mut engine = DecisionEngine::new();
    assert!(engine
        .register_node(StaticDecision {
            name: "deny",
            outcome: DecisionOutcome::Deny("blocked".to_string()),
        })
        .is_ok());
    let command = command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = engine.evaluate(&command, &context());
    assert!(result.is_ok());
    assert_eq!(
        result.ok(),
        Some(DecisionOutcome::Deny("blocked".to_string()))
    );
}

#[test]
fn decision_engine_defer_blocks_for_now() {
    let mut engine = DecisionEngine::new();
    assert!(engine
        .register_node(StaticDecision {
            name: "defer",
            outcome: DecisionOutcome::Defer("later".to_string()),
        })
        .is_ok());
    let command = command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = engine.evaluate(&command, &context());
    assert!(result.is_ok());
    assert_eq!(
        result.ok(),
        Some(DecisionOutcome::Defer("later".to_string()))
    );
}

#[test]
fn decision_engine_respects_node_order() {
    let mut engine = DecisionEngine::new();
    assert!(engine
        .register_node(StaticDecision {
            name: "first",
            outcome: DecisionOutcome::Defer("first".to_string()),
        })
        .is_ok());
    assert!(engine
        .register_node(StaticDecision {
            name: "second",
            outcome: DecisionOutcome::Deny("second".to_string()),
        })
        .is_ok());
    let command = command();
    assert!(command.is_ok());
    let command = match command {
        Ok(command) => command,
        Err(_) => return,
    };
    let result = engine.evaluate(&command, &context());
    assert!(result.is_ok());
    assert_eq!(
        result.ok(),
        Some(DecisionOutcome::Defer("first".to_string()))
    );
}
