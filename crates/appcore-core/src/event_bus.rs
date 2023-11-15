// =============================================================================
//        #######
//     ###       ###     F: event_bus.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded in-memory event bus for emitted command events.

use crate::envelope::EventEnvelope;
use crate::operational_journal::FileOperationalJournal;
use parking_lot::Mutex;
use std::sync::Arc;

/// Bounded process-local store of recently emitted Runtime events.
#[derive(Debug, Default)]
pub struct EventBus {
    events: Mutex<Vec<EventEnvelope>>,
    journal: Mutex<Option<Arc<FileOperationalJournal>>>,
    journal_error: Mutex<Option<String>>,
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        let guard = self.events.lock();
        Self {
            events: Mutex::new(guard.clone()),
            journal: Mutex::new(self.journal.lock().clone()),
            journal_error: Mutex::new(self.journal_error.lock().clone()),
        }
    }
}

impl EventBus {
    /// Creates an empty event bus.
    pub fn new() -> Self {
        Self::default()
    }

    /// Attaches a durable journal and loads its retained event envelopes.
    pub fn attach_journal(&self, journal: Arc<FileOperationalJournal>) {
        let mut events = journal.events();
        if events.len() > 10_000 {
            events.drain(..events.len() - 10_000);
        }
        *self.events.lock() = events;
        *self.journal.lock() = Some(journal);
        *self.journal_error.lock() = None;
    }

    /// Returns the last durable journal failure, when persistence degraded.
    pub fn durability_error(&self) -> Option<String> {
        self.journal_error.lock().clone()
    }

    /// Appends one event and evicts the oldest item at the configured bound.
    pub fn emit(&self, event: EventEnvelope) {
        self.persist(&event);
        let mut guard = self.events.lock();
        if guard.len() >= 10000 {
            let to_remove = guard.len().saturating_sub(9999);
            if to_remove > 0 {
                guard.drain(0..to_remove);
            }
        }
        guard.push(event);
    }

    /// Appends multiple events and retains at most 10,000 recent items.
    pub fn emit_many(&self, events: Vec<EventEnvelope>) {
        for event in &events {
            self.persist(event);
        }
        let mut guard = self.events.lock();
        guard.extend(events);
        if guard.len() > 10000 {
            let to_remove = guard.len().saturating_sub(10000);
            if to_remove > 0 {
                guard.drain(0..to_remove);
            }
        }
    }

    /// Returns the current number of retained events.
    pub fn len(&self) -> usize {
        self.events.lock().len()
    }

    /// Reports whether no events are retained.
    pub fn is_empty(&self) -> bool {
        self.events.lock().is_empty()
    }

    /// Returns a point-in-time copy of retained events.
    pub fn events(&self) -> Vec<EventEnvelope> {
        self.events.lock().clone()
    }

    /// Removes all retained events.
    pub fn clear(&self) {
        self.events.lock().clear();
    }

    fn persist(&self, event: &EventEnvelope) {
        if let Some(journal) = self.journal.lock().clone() {
            if let Err(error) = journal.append_event(event.clone()) {
                *self.journal_error.lock() = Some(crate::redact_text(&format!("{error:?}")));
            }
        }
    }
}

#[cfg(test)]
#[path = "event_bus_tests.rs"]
mod tests;
