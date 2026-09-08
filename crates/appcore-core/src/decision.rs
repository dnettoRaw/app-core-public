// =============================================================================
//        #######
//     ###       ###     F: decision.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Decision contracts, name catalog, and execution engine.

use std::fmt;

use crate::context::RuntimeContext;
use crate::envelope::CommandEnvelope;
use crate::error::{RuntimeError, RuntimeResult};
use crate::registry::NameRegistry;

/// Maximum decision nodes or declared decision names in one owner.
pub const MAX_DECISION_NODES: usize = 4096;
/// Maximum UTF-8 bytes in one decision name.
pub const MAX_DECISION_NAME_BYTES: usize = 256;

/// Decision result produced by a node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DecisionOutcome {
    /// Continue evaluation or allow execution.
    Allow,
    /// Reject execution with a controlled reason.
    Deny(String),
    /// Defer execution with a controlled reason.
    Defer(String),
}

/// Policy decision node contract.
pub trait DecisionNode: Send + Sync {
    /// Returns the stable policy node name.
    fn name(&self) -> &str;
    /// Evaluates one command without mutating Runtime infrastructure.
    fn decide(
        &self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<DecisionOutcome>;
}

/// Ordered catalog of declared policy node names.
#[derive(Debug, Default)]
pub struct DecisionRegistry {
    names: NameRegistry<String>,
}

impl DecisionRegistry {
    /// Creates an empty decision registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers the name exposed by a decision node.
    pub fn register<N>(&mut self, node: &N) -> RuntimeResult<()>
    where
        N: DecisionNode,
    {
        self.register_name(node.name())
    }

    /// Registers a decision node name directly.
    pub fn register_name(&mut self, name: &str) -> RuntimeResult<()> {
        validate_registration(name, self.names.list())?;
        self.names.register(name.to_string(), "decision")
    }

    /// Reports whether a decision node name is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.names.list().iter().any(|item| item == name)
    }

    /// Returns the number of registered node names.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Reports whether no node names are registered.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Returns node names in registration order.
    pub fn list(&self) -> &[String] {
        self.names.list()
    }
}

/// Sequential policy engine that short-circuits on deny or defer.
#[derive(Default)]
pub struct DecisionEngine {
    nodes: Vec<Box<dyn DecisionNode + Send + Sync>>,
    node_names: Vec<String>,
}

impl fmt::Debug for DecisionEngine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DecisionEngine")
            .field("node_count", &self.nodes.len())
            .field("node_names", &self.node_names)
            .finish()
    }
}

impl DecisionEngine {
    /// Creates an empty decision engine that allows by default.
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends one uniquely named node within the count and name-byte limits.
    /// Rejection leaves evaluation order and existing nodes unchanged.
    pub fn register_node<N: DecisionNode + 'static>(&mut self, node: N) -> RuntimeResult<()> {
        let name = node.name();
        validate_registration(name, &self.node_names)?;
        self.node_names.push(name.to_string());
        self.nodes.push(Box::new(node));
        Ok(())
    }

    /// Returns the number of decision nodes.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Reports whether no decision nodes are configured.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Returns decision node names in evaluation order.
    pub fn node_names(&self) -> &[String] {
        &self.node_names
    }

    /// Evaluates nodes in order and returns the first non-allow outcome.
    pub fn evaluate(
        &self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<DecisionOutcome> {
        if self.nodes.is_empty() {
            return Ok(DecisionOutcome::Allow);
        }

        for node in &self.nodes {
            match node.decide(command, context)? {
                DecisionOutcome::Allow => {}
                DecisionOutcome::Deny(message) => return Ok(DecisionOutcome::Deny(message)),
                DecisionOutcome::Defer(message) => return Ok(DecisionOutcome::Defer(message)),
            }
        }

        Ok(DecisionOutcome::Allow)
    }
}

fn validate_registration(name: &str, existing: &[String]) -> RuntimeResult<()> {
    if name.is_empty() || name.len() > MAX_DECISION_NAME_BYTES {
        return Err(RuntimeError::InvalidRequest {
            kind: "decision",
            reason: "decision name must contain 1..=256 UTF-8 bytes",
        });
    }
    if existing.iter().any(|registered| registered == name) {
        return Err(RuntimeError::DuplicateRegistryItem {
            kind: "decision".into(),
        });
    }
    if existing.len() >= MAX_DECISION_NODES {
        return Err(RuntimeError::InvalidRequest {
            kind: "decision",
            reason: "decision registration capacity exceeded",
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "decision_tests.rs"]
mod tests;
