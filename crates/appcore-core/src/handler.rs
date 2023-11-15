// =============================================================================
//        #######
//     ###       ###     F: handler.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Command handler contracts without execution engine concerns.

use crate::context::RuntimeContext;
use crate::envelope::{CommandEnvelope, EventEnvelope};
use crate::error::RuntimeResult;
use crate::ids::CommandName;

/// Structured command handling result.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CommandResult {
    accepted: bool,
    events: Vec<EventEnvelope>,
    message: Option<String>,
}

impl CommandResult {
    /// Creates an accepted result with emitted fact events.
    pub fn accepted(events: Vec<EventEnvelope>) -> Self {
        Self {
            accepted: true,
            events,
            message: None,
        }
    }

    /// Creates a controlled rejection without emitted events.
    pub fn rejected(message: impl Into<String>) -> Self {
        Self {
            accepted: false,
            events: Vec::new(),
            message: Some(message.into()),
        }
    }

    /// Reports whether the command was accepted.
    pub fn is_accepted(&self) -> bool {
        self.accepted
    }

    /// Returns events emitted by an accepted command.
    pub fn events(&self) -> &[EventEnvelope] {
        &self.events
    }

    /// Returns the controlled rejection message, when present.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }
}

/// Command handler contract.
pub trait CommandHandler: Send + Sync {
    /// Returns the command name handled by this implementation.
    fn command_name(&self) -> CommandName;

    /// Handles one validated command in a read-only Runtime context.
    fn handle(
        &self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult>;
}

#[cfg(test)]
#[path = "handler_tests.rs"]
mod tests;
