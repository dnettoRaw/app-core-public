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
use crate::operational_journal::{FileOperationalJournal, OperationalJournalRecord};
use crate::TraceContext;
use parking_lot::Mutex;
use serde::ser::SerializeSeq;
use serde::{Serialize, Serializer};
use std::collections::VecDeque;
use std::sync::Arc;

const MAX_RETAINED_EVENTS: usize = 10_000;
/// Default aggregate memory budget for process-local emitted events.
pub const DEFAULT_EVENT_BUS_MAX_BYTES: usize = 16 * 1024 * 1024;

/// Point-in-time pressure metrics for a process-local event bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventBusStats {
    /// Retained event envelopes.
    pub event_count: usize,
    /// Estimated bytes retained by the current snapshot.
    pub used_bytes: usize,
    /// Highest retained byte count observed by this bus.
    pub peak_bytes: usize,
    /// Events evicted to maintain count or byte limits.
    pub evictions: u64,
    /// Individual events too large for the configured byte budget.
    pub rejections: u64,
    /// Aggregate configured byte budget.
    pub max_bytes: usize,
}

/// Shared immutable point-in-time view of emitted events.
#[derive(Clone, Debug)]
pub struct EventBusSnapshot {
    events: Arc<VecDeque<Arc<OperationalJournalRecord>>>,
}

impl EventBusSnapshot {
    /// Returns the number of events captured by this snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Reports whether this snapshot contains no events.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Iterates over at most the newest `limit` events without cloning them.
    pub fn recent(&self, limit: usize) -> impl Iterator<Item = &EventEnvelope> {
        let start = self.events.len().saturating_sub(limit);
        self.events.iter().skip(start).filter_map(event_record)
    }
}

impl Serialize for EventBusSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(Some(self.events.len()))?;
        for event in self.events.iter().filter_map(event_record) {
            sequence.serialize_element(event)?;
        }
        sequence.end()
    }
}

#[derive(Debug, Clone)]
struct EventBusState {
    events: Arc<VecDeque<Arc<OperationalJournalRecord>>>,
    used_bytes: usize,
    peak_bytes: usize,
    evictions: u64,
    rejections: u64,
    max_bytes: usize,
}

impl EventBusState {
    fn new(max_bytes: usize) -> Self {
        Self {
            events: Arc::default(),
            used_bytes: 0,
            peak_bytes: 0,
            evictions: 0,
            rejections: 0,
            max_bytes: max_bytes.max(1),
        }
    }

    fn push_shared(&mut self, record: Arc<OperationalJournalRecord>) {
        let Some(event) = event_record(&record) else {
            self.rejections = self.rejections.saturating_add(1);
            return;
        };
        let bytes = event_retained_bytes(event);
        if bytes > self.max_bytes {
            self.rejections = self.rejections.saturating_add(1);
            return;
        }
        while self.events.len() >= MAX_RETAINED_EVENTS {
            self.pop_front();
        }
        while self.used_bytes.saturating_add(bytes) > self.max_bytes {
            if !self.pop_front() {
                self.rejections = self.rejections.saturating_add(1);
                return;
            }
        }
        Arc::make_mut(&mut self.events).push_back(record);
        self.used_bytes = self.used_bytes.saturating_add(bytes);
        self.peak_bytes = self.peak_bytes.max(self.used_bytes);
    }

    fn pop_front(&mut self) -> bool {
        let Some(event) = Arc::make_mut(&mut self.events).pop_front() else {
            return false;
        };
        if let Some(event) = event_record(&event) {
            self.used_bytes = self.used_bytes.saturating_sub(event_retained_bytes(event));
        }
        self.evictions = self.evictions.saturating_add(1);
        true
    }

    fn replace(&mut self, events: Vec<Arc<OperationalJournalRecord>>) {
        self.clear();
        for event in events {
            self.push_shared(event);
        }
    }

    fn clear(&mut self) {
        self.events = Arc::default();
        self.used_bytes = 0;
    }

    fn stats(&self) -> EventBusStats {
        EventBusStats {
            event_count: self.events.len(),
            used_bytes: self.used_bytes,
            peak_bytes: self.peak_bytes,
            evictions: self.evictions,
            rejections: self.rejections,
            max_bytes: self.max_bytes,
        }
    }
}

/// Bounded process-local store of recently emitted Runtime events.
#[derive(Debug)]
pub struct EventBus {
    state: Mutex<EventBusState>,
    journal: Mutex<Option<Arc<FileOperationalJournal>>>,
    journal_error: Mutex<Option<String>>,
}

impl Default for EventBus {
    fn default() -> Self {
        Self::with_max_bytes(DEFAULT_EVENT_BUS_MAX_BYTES)
    }
}

impl Clone for EventBus {
    fn clone(&self) -> Self {
        Self {
            state: Mutex::new(self.state.lock().clone()),
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

    /// Creates an empty event bus with an aggregate retained-byte budget.
    pub fn with_max_bytes(max_bytes: usize) -> Self {
        Self {
            state: Mutex::new(EventBusState::new(max_bytes)),
            journal: Mutex::new(None),
            journal_error: Mutex::new(None),
        }
    }

    /// Attaches a durable journal and shares its retained immutable event records.
    pub fn attach_journal(&self, journal: Arc<FileOperationalJournal>) {
        self.state.lock().replace(journal.shared_event_records());
        *self.journal.lock() = Some(journal);
        *self.journal_error.lock() = None;
    }

    /// Returns the last durable journal failure, when persistence degraded.
    pub fn durability_error(&self) -> Option<String> {
        self.journal_error.lock().clone()
    }

    /// Appends one event, sharing its record with an attached durable journal.
    pub fn emit(&self, event: EventEnvelope) {
        let event = Arc::new(OperationalJournalRecord::Event(event));
        self.persist(Arc::clone(&event));
        self.state.lock().push_shared(event);
    }

    /// Appends events and retains them within the count and aggregate-byte bounds.
    pub fn emit_many(&self, events: Vec<EventEnvelope>) {
        let events = events
            .into_iter()
            .map(|event| Arc::new(OperationalJournalRecord::Event(event)))
            .collect::<Vec<_>>();
        for event in &events {
            self.persist(Arc::clone(event));
        }
        let mut state = self.state.lock();
        for event in events {
            state.push_shared(event);
        }
    }

    /// Returns the current number of retained events.
    pub fn len(&self) -> usize {
        self.state.lock().events.len()
    }

    /// Reports whether no events are retained.
    pub fn is_empty(&self) -> bool {
        self.state.lock().events.is_empty()
    }

    /// Returns a point-in-time copy of retained events.
    pub fn events(&self) -> Vec<EventEnvelope> {
        self.snapshot().recent(usize::MAX).cloned().collect()
    }

    /// Returns a shared immutable snapshot without cloning event fields.
    pub fn snapshot(&self) -> EventBusSnapshot {
        EventBusSnapshot {
            events: Arc::clone(&self.state.lock().events),
        }
    }

    /// Returns current count, byte-pressure, eviction, and rejection metrics.
    pub fn stats(&self) -> EventBusStats {
        self.state.lock().stats()
    }

    /// Removes all retained events.
    pub fn clear(&self) {
        self.state.lock().clear();
    }

    fn persist(&self, event: Arc<OperationalJournalRecord>) {
        if let Some(journal) = self.journal.lock().clone() {
            if let Err(error) = journal.append_shared_event(event) {
                *self.journal_error.lock() = Some(crate::redact_text(&format!("{error:?}")));
            }
        }
    }
}

fn event_record(record: &Arc<OperationalJournalRecord>) -> Option<&EventEnvelope> {
    match record.as_ref() {
        OperationalJournalRecord::Event(event) => Some(event),
        OperationalJournalRecord::Audit(_) => None,
    }
}

fn event_retained_bytes(event: &EventEnvelope) -> usize {
    std::mem::size_of::<EventEnvelope>()
        .saturating_add(event.event_name.as_str().len())
        .saturating_add(event.event_id.len())
        .saturating_add(event.app_id.as_str().len())
        .saturating_add(event.node_id.as_str().len())
        .saturating_add(event.payload.len())
        .saturating_add(trace_retained_bytes(event.trace.as_ref()))
}

fn trace_retained_bytes(trace: Option<&TraceContext>) -> usize {
    let Some(trace) = trace else {
        return 0;
    };
    std::mem::size_of::<TraceContext>()
        .saturating_add(trace.trace_id.len())
        .saturating_add(trace.span_id.len())
        .saturating_add(trace.parent_span_id.as_ref().map_or(0, String::len))
        .saturating_add(trace.originating_core_id.as_str().len())
        .saturating_add(trace.current_core_id.as_str().len())
        .saturating_add(trace.tenant_id.as_str().len())
        .saturating_add(trace.command_id.as_ref().map_or(0, String::len))
}

#[cfg(test)]
#[path = "event_bus_tests.rs"]
mod tests;
