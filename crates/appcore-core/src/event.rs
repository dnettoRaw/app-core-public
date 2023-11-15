// =============================================================================
//        #######
//     ###       ###     F: event.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Event contract and ordered, duplicate-safe event registry.
//! Event payload transport uses `EventEnvelope`.
//! Handler outcomes may emit events through `CommandResult`.

use crate::error::RuntimeResult;
use crate::ids::EventName;
use crate::registry::NameRegistry;

/// Minimal runtime event contract.
pub trait RuntimeEvent {
    /// Returns the stable event name used for registration.
    fn name(&self) -> &EventName;
}

/// Ordered registry of declared event names.
#[derive(Debug, Default)]
pub struct EventRegistry {
    names: NameRegistry<EventName>,
}

impl EventRegistry {
    /// Creates an empty event registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers an event name, rejecting duplicates.
    pub fn register(&mut self, name: EventName) -> RuntimeResult<()> {
        self.names.register(name, "event")
    }

    /// Reports whether an event name is registered.
    pub fn contains(&self, name: &EventName) -> bool {
        self.names.contains(name)
    }

    /// Returns the number of registered event names.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Reports whether no event names are registered.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Returns event names in registration order.
    pub fn list(&self) -> &[EventName] {
        self.names.list()
    }
}

#[cfg(test)]
#[path = "event_tests.rs"]
mod tests;
