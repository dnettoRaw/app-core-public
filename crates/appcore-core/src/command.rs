// =============================================================================
//        #######
//     ###       ###     F: command.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Command contract and ordered, duplicate-safe command registry.
//! Command payload transport uses `CommandEnvelope`.
//! Handler contract lives in `handler.rs`.

use crate::error::RuntimeResult;
use crate::ids::CommandName;
use crate::registry::NameRegistry;

/// Minimal runtime command contract.
pub trait RuntimeCommand {
    /// Returns the stable command name used for registration and dispatch.
    fn name(&self) -> &CommandName;
}

/// Ordered registry of declared command names.
#[derive(Debug, Default)]
pub struct CommandRegistry {
    names: NameRegistry<CommandName>,
}

impl CommandRegistry {
    /// Creates an empty command registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a command name, rejecting duplicates.
    pub fn register(&mut self, name: CommandName) -> RuntimeResult<()> {
        self.names.register(name, "command")
    }

    /// Reports whether a command name is registered.
    pub fn contains(&self, name: &CommandName) -> bool {
        self.names.contains(name)
    }

    /// Returns the number of registered command names.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Reports whether no command names are registered.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Returns command names in registration order.
    pub fn list(&self) -> &[CommandName] {
        self.names.list()
    }
}

#[cfg(test)]
#[path = "command_tests.rs"]
mod tests;
