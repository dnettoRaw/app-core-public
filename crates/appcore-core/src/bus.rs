// =============================================================================
//        #######
//     ###       ###     F: bus.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Minimal synchronous command bus contract.

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::context::RuntimeContext;
use crate::envelope::CommandEnvelope;
use crate::error::{RuntimeError, RuntimeResult};
use crate::handler::{CommandHandler, CommandResult};
use crate::ids::CommandName;

/// In-memory command bus that routes envelopes to registered handlers.
#[derive(Default)]
pub struct CommandBus {
    handlers: HashMap<CommandName, Arc<dyn CommandHandler + Send + Sync>>,
}

impl fmt::Debug for CommandBus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CommandBus")
            .field("handler_count", &self.handlers.len())
            .finish()
    }
}

impl CommandBus {
    /// Creates an empty command bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one handler, rejecting duplicate command names.
    pub fn register_handler<H: CommandHandler + 'static>(
        &mut self,
        handler: H,
    ) -> RuntimeResult<()> {
        let name = handler.command_name();
        if self.handlers.contains_key(&name) {
            return Err(RuntimeError::HandlerAlreadyRegistered(name));
        }

        self.handlers.insert(name, Arc::new(handler));
        Ok(())
    }

    /// Reports whether a handler exists for `name`.
    pub fn contains_handler(&self, name: &CommandName) -> bool {
        self.handlers.contains_key(name)
    }

    /// Returns the number of registered handlers.
    pub fn len(&self) -> usize {
        self.handlers.len()
    }

    /// Reports whether no handlers are registered.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Dispatches an envelope to the handler matching its command name.
    pub fn dispatch(
        &self,
        command: &CommandEnvelope,
        context: &dyn RuntimeContext,
    ) -> RuntimeResult<CommandResult> {
        let name = command.command_name();
        let Some(handler) = self.handlers.get(name) else {
            return Err(RuntimeError::HandlerNotFound(name.clone()));
        };

        handler.handle(command, context)
    }
}

#[cfg(test)]
#[path = "bus_tests.rs"]
mod tests;
