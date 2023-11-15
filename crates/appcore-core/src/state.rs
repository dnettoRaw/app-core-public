// =============================================================================
//        #######
//     ###       ###     F: state.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! State contracts: registry plus a minimal deterministic state machine.

use crate::error::{RuntimeError, RuntimeResult};
use crate::ids::{EventName, StateName};
use crate::registry::NameRegistry;

/// Minimal runtime state contract.
pub trait RuntimeState {
    /// Returns the stable state name.
    fn name(&self) -> &StateName;
}

/// Explicit state transition definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateTransition {
    /// Source state.
    pub from: StateName,
    /// Event that triggers the transition.
    pub event: EventName,
    /// Destination state.
    pub to: StateName,
}

/// Minimal state machine based on explicit transitions.
#[derive(Debug, Clone)]
pub struct StateMachine {
    current: StateName,
    transitions: Vec<StateTransition>,
}

impl StateMachine {
    /// Creates a state machine without registered transitions.
    pub fn new(initial: StateName) -> Self {
        Self {
            current: initial,
            transitions: Vec::new(),
        }
    }

    /// Returns the current state.
    pub fn current(&self) -> &StateName {
        &self.current
    }

    /// Registers one deterministic transition, rejecting duplicate source/event pairs.
    pub fn add_transition(&mut self, transition: StateTransition) -> RuntimeResult<()> {
        let duplicated = self
            .transitions
            .iter()
            .any(|existing| existing.from == transition.from && existing.event == transition.event);
        if duplicated {
            return Err(RuntimeError::DuplicateStateTransition);
        }

        self.transitions.push(transition);
        Ok(())
    }

    /// Reports whether `event` can be applied from the current state.
    pub fn can_apply(&self, event: &EventName) -> bool {
        self.transitions
            .iter()
            .any(|transition| transition.from == self.current && &transition.event == event)
    }

    /// Applies `event` and returns the new current state.
    pub fn apply(&mut self, event: &EventName) -> RuntimeResult<&StateName> {
        let transition = self
            .transitions
            .iter()
            .find(|transition| transition.from == self.current && &transition.event == event);

        let Some(transition) = transition else {
            return Err(RuntimeError::InvalidStateTransition);
        };

        self.current = transition.to.clone();
        Ok(&self.current)
    }

    /// Returns all transitions in registration order.
    pub fn transitions(&self) -> &[StateTransition] {
        &self.transitions
    }
}

/// Ordered registry of declared state names.
#[derive(Debug, Default)]
pub struct StateRegistry {
    names: NameRegistry<StateName>,
}

impl StateRegistry {
    /// Creates an empty state registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a state name, rejecting duplicates.
    pub fn register(&mut self, name: StateName) -> RuntimeResult<()> {
        self.names.register(name, "state")
    }

    /// Reports whether a state name is registered.
    pub fn contains(&self, name: &StateName) -> bool {
        self.names.contains(name)
    }

    /// Returns the number of registered state names.
    pub fn len(&self) -> usize {
        self.names.len()
    }

    /// Reports whether no states are registered.
    pub fn is_empty(&self) -> bool {
        self.names.is_empty()
    }

    /// Returns state names in registration order.
    pub fn list(&self) -> &[StateName] {
        self.names.list()
    }
}

#[cfg(test)]
#[path = "state_tests.rs"]
mod tests;
