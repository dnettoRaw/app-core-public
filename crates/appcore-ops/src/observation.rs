// =============================================================================
//        #######
//     ###       ###     F: observation.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/07/21 23:21:21 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Bounded, tool-independent runtime observation events.

use appcore_core::redact_text_with_limit;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

/// Maximum retained attributes per observation.
pub const MAX_OBSERVATION_ATTRIBUTES: usize = 32;
/// Maximum UTF-8 bytes retained in an observation name.
pub const MAX_OBSERVATION_NAME_BYTES: usize = 128;
/// Maximum UTF-8 bytes retained in an observation attribute key.
pub const MAX_OBSERVATION_KEY_BYTES: usize = 64;
/// Maximum UTF-8 bytes retained in an observation attribute value.
pub const MAX_OBSERVATION_VALUE_BYTES: usize = 1_024;
/// Maximum UTF-8 bytes retained in a trace identifier.
pub const MAX_OBSERVATION_TRACE_BYTES: usize = 256;
/// Maximum events retained by one process-local observation sink.
pub const MAX_IN_MEMORY_OBSERVATION_ITEMS: usize = 65_536;
/// Absolute aggregate retained-byte ceiling for one process-local sink.
pub const MAX_IN_MEMORY_OBSERVATION_BYTES: usize = 16 * 1024 * 1024;
/// Maximum operational drains attached to one process-local observation sink.
pub const MAX_OBSERVATION_DRAINS: usize = 32;
const OBSERVATION_FIXED_BYTES: usize = std::mem::size_of::<ObservationEvent>()
    + std::mem::size_of::<Arc<ObservationEvent>>()
    + std::mem::size_of::<usize>() * 2;
const ATTRIBUTE_FIXED_BYTES: usize =
    std::mem::size_of::<(String, String)>() + std::mem::size_of::<usize>() * 4;

/// Runtime subsystem that produced an observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    /// Process lifecycle.
    Lifecycle,
    /// Runtime or deployment configuration.
    Configuration,
    /// Health evaluation.
    Health,
    /// Authentication, authorization or secret boundary.
    Security,
    /// Storage operation.
    Storage,
    /// Control-plane operation.
    ControlPlane,
    /// Direct peer RPC operation.
    PeerRpc,
    /// Scheduler operation.
    Scheduler,
    /// Synchronization operation.
    Sync,
    /// Audit operation.
    Audit,
    /// Diagnostic operation.
    Diagnostic,
}

/// Severity of one runtime observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObservationSeverity {
    /// Developer diagnostic fact.
    Debug,
    /// Normal operational fact.
    Info,
    /// Recoverable or degraded condition.
    Warning,
    /// Failed operation.
    Error,
}

/// Generic runtime fact suitable for logs, diagnostics, metrics and audit sinks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservationEvent {
    /// Runtime subsystem category.
    pub kind: ObservationKind,
    /// Observation severity.
    pub severity: ObservationSeverity,
    /// Stable observation name.
    pub name: String,
    /// Timestamp in Unix milliseconds.
    pub timestamp_ms: u64,
    /// Optional trace identity.
    pub trace_id: Option<String>,
    /// Bounded non-sensitive dimensions.
    pub attributes: BTreeMap<String, String>,
}

impl ObservationEvent {
    /// Creates an event without attributes.
    pub fn new(
        kind: ObservationKind,
        severity: ObservationSeverity,
        name: impl Into<String>,
        timestamp_ms: u64,
    ) -> Self {
        Self {
            kind,
            severity,
            name: name.into(),
            timestamp_ms,
            trace_id: None,
            attributes: BTreeMap::new(),
        }
    }

    /// Attaches a trace identity.
    pub fn with_trace_id(mut self, trace_id: impl Into<String>) -> Self {
        self.trace_id = Some(trace_id.into());
        self
    }

    /// Adds one attribute. Sensitive keys and values are redacted immediately.
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        let key = key.into();
        let sensitive = is_sensitive_key(&key);
        let key = redact_text_with_limit(&key, MAX_OBSERVATION_KEY_BYTES);
        if self.attributes.len() >= MAX_OBSERVATION_ATTRIBUTES
            && !self.attributes.contains_key(&key)
        {
            return self;
        }
        let value = if sensitive {
            "[REDACTED]".to_string()
        } else {
            redact_text_with_limit(&value.into(), MAX_OBSERVATION_VALUE_BYTES)
        };
        self.attributes.insert(key, value);
        self
    }

    pub(crate) fn redacted(mut self) -> Self {
        self.name = redact_text_with_limit(&self.name, MAX_OBSERVATION_NAME_BYTES);
        self.name.shrink_to_fit();
        self.trace_id = self.trace_id.map(|value| {
            let mut value = redact_text_with_limit(&value, MAX_OBSERVATION_TRACE_BYTES);
            value.shrink_to_fit();
            value
        });
        let mut attributes = BTreeMap::new();
        for (key, value) in std::mem::take(&mut self.attributes)
            .into_iter()
            .take(MAX_OBSERVATION_ATTRIBUTES)
        {
            let sensitive = is_sensitive_key(&key);
            let mut key = redact_text_with_limit(&key, MAX_OBSERVATION_KEY_BYTES);
            let mut value = if sensitive {
                "[REDACTED]".to_string()
            } else {
                redact_text_with_limit(&value, MAX_OBSERVATION_VALUE_BYTES)
            };
            key.shrink_to_fit();
            value.shrink_to_fit();
            attributes.insert(key, value);
        }
        self.attributes = attributes;
        self
    }
}

/// Redacted, bounded observation payload shared across multiple sinks.
///
/// Construction reapplies the same validation as [`ObservationSink::emit`],
/// so a sink may retain the immutable payload without copying its owned fields.
#[derive(Debug, Clone)]
pub struct SharedObservationEvent {
    event: Arc<ObservationEvent>,
}

impl SharedObservationEvent {
    /// Redacts, bounds and shares one observation event.
    pub fn new(event: ObservationEvent) -> Self {
        Self {
            event: Arc::new(event.redacted()),
        }
    }

    /// Borrows the validated observation payload.
    pub fn as_event(&self) -> &ObservationEvent {
        &self.event
    }

    fn clone_event_arc(&self) -> Arc<ObservationEvent> {
        Arc::clone(&self.event)
    }
}

/// Destination for generic observation events.
pub trait ObservationSink: Send + Sync {
    /// Emits one event without blocking on external tooling.
    fn emit(&self, event: ObservationEvent);

    /// Emits an already validated shared event.
    ///
    /// Existing sinks remain compatible through this owned fallback. Sinks
    /// that retain or only inspect events should override it to avoid copying
    /// the payload.
    fn emit_shared(&self, event: &SharedObservationEvent) {
        self.emit(event.as_event().clone());
    }
}

/// Immutable shared view of retained observations, ordered oldest to newest.
#[derive(Debug, Clone, Default)]
pub struct ObservationSnapshot {
    events: Arc<VecDeque<Arc<ObservationEvent>>>,
}

impl ObservationSnapshot {
    /// Returns the number of observations in the snapshot.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    /// Reports whether the snapshot contains no observations.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Iterates from the oldest observation to the newest without cloning it.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &ObservationEvent> {
        self.events.iter().map(AsRef::as_ref)
    }

    /// Iterates over at most the newest `limit` observations in chronological order.
    pub fn recent(&self, limit: usize) -> impl Iterator<Item = &ObservationEvent> {
        self.events
            .iter()
            .skip(self.events.len().saturating_sub(limit))
            .map(AsRef::as_ref)
    }
}

/// Point-in-time retention pressure for a process-local observation sink.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct InMemoryObservationPressure {
    /// Current retained event count.
    pub entries: usize,
    /// Configured retained-event ceiling.
    pub max_entries: usize,
    /// Estimated bytes retained by events and their owned fields.
    pub used_bytes: usize,
    /// Highest estimated retained byte count observed since creation.
    pub peak_bytes: usize,
    /// Configured aggregate retained-byte ceiling.
    pub max_bytes: usize,
    /// Oldest events discarded to enforce count or byte limits.
    pub evictions: u64,
    /// Events not retained because one event exceeded the byte ceiling.
    pub oversized_rejections: u64,
    /// Drains rejected because the attachment ceiling was full.
    pub drain_rejections: u64,
}

#[derive(Debug)]
struct ObservationState {
    events: Arc<VecDeque<Arc<ObservationEvent>>>,
    pressure: InMemoryObservationPressure,
}

type ObservationDrain = Arc<dyn ObservationSink>;
type ObservationDrainGeneration = Arc<Vec<ObservationDrain>>;

/// Bounded in-memory sink used by diagnostics and embedded runtimes.
///
/// Drain configuration is copy-on-write. Emission borrows one immutable drain
/// generation and releases its configuration lock before invoking callbacks.
#[derive(Clone)]
pub struct InMemoryObservationSink {
    capacity: usize,
    state: Arc<Mutex<ObservationState>>,
    drains: Arc<RwLock<ObservationDrainGeneration>>,
}

impl std::fmt::Debug for InMemoryObservationSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InMemoryObservationSink")
            .field("capacity", &self.capacity)
            .field("event_count", &self.len())
            .field("pressure", &self.pressure())
            .field("drain_count", &self.drains.read().len())
            .finish()
    }
}

impl InMemoryObservationSink {
    /// Creates a sink that retains the newest `capacity` events.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.clamp(1, MAX_IN_MEMORY_OBSERVATION_ITEMS);
        let max_bytes = default_max_bytes(capacity);
        Self::with_limits(capacity, max_bytes)
    }

    /// Creates a sink with a tighter aggregate retained-byte ceiling.
    ///
    /// Count is clamped to the public safety ceiling. The byte limit is clamped
    /// to `1..=default_max_bytes(capacity)` and is observable through
    /// [`Self::pressure`].
    pub fn with_max_bytes(capacity: usize, max_bytes: usize) -> Self {
        let capacity = capacity.clamp(1, MAX_IN_MEMORY_OBSERVATION_ITEMS);
        Self::with_limits(capacity, max_bytes.clamp(1, default_max_bytes(capacity)))
    }

    fn with_limits(capacity: usize, max_bytes: usize) -> Self {
        Self {
            capacity,
            state: Arc::new(Mutex::new(ObservationState {
                events: Arc::new(VecDeque::new()),
                pressure: InMemoryObservationPressure {
                    max_entries: capacity,
                    max_bytes,
                    ..InMemoryObservationPressure::default()
                },
            })),
            drains: Arc::new(RwLock::new(Arc::new(Vec::new()))),
        }
    }

    /// Adds an operational drain that receives future redacted events.
    pub fn add_drain(&self, drain: Arc<dyn ObservationSink>) {
        let _ = self.try_add_drain(drain);
    }

    /// Attempts to add an operational drain under the attachment ceiling.
    pub fn try_add_drain(&self, drain: Arc<dyn ObservationSink>) -> bool {
        let accepted = {
            let mut drains = self.drains.write();
            if drains.len() >= MAX_OBSERVATION_DRAINS {
                false
            } else {
                Arc::make_mut(&mut drains).push(drain);
                true
            }
        };
        if !accepted {
            let mut state = self.state.lock();
            state.pressure.drain_rejections = state.pressure.drain_rejections.saturating_add(1);
        }
        accepted
    }

    /// Returns the number of attached operational drains.
    pub fn drain_count(&self) -> usize {
        self.drains.read().len()
    }

    /// Returns a stable snapshot from oldest to newest.
    pub fn snapshot(&self) -> Vec<ObservationEvent> {
        self.shared_snapshot().iter().cloned().collect()
    }

    /// Returns an immutable snapshot without cloning retained observations.
    pub fn shared_snapshot(&self) -> ObservationSnapshot {
        ObservationSnapshot {
            events: Arc::clone(&self.state.lock().events),
        }
    }

    /// Returns current count and retained-memory pressure.
    pub fn pressure(&self) -> InMemoryObservationPressure {
        self.state.lock().pressure
    }

    /// Returns the number of retained events.
    pub fn len(&self) -> usize {
        self.state.lock().pressure.entries
    }

    /// Reports whether no events are retained.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for InMemoryObservationSink {
    fn default() -> Self {
        Self::new(1_024)
    }
}

impl ObservationSink for InMemoryObservationSink {
    fn emit(&self, event: ObservationEvent) {
        self.emit_shared(&SharedObservationEvent::new(event));
    }

    fn emit_shared(&self, event: &SharedObservationEvent) {
        let retained_bytes = observation_retained_bytes(event.as_event());
        let mut state = self.state.lock();
        if retained_bytes > state.pressure.max_bytes {
            state.pressure.oversized_rejections =
                state.pressure.oversized_rejections.saturating_add(1);
        } else {
            while state.pressure.entries >= self.capacity
                || state.pressure.used_bytes.saturating_add(retained_bytes)
                    > state.pressure.max_bytes
            {
                let Some(removed) = Arc::make_mut(&mut state.events).pop_front() else {
                    break;
                };
                state.pressure.entries = state.pressure.entries.saturating_sub(1);
                state.pressure.used_bytes = state
                    .pressure
                    .used_bytes
                    .saturating_sub(observation_retained_bytes(&removed));
                state.pressure.evictions = state.pressure.evictions.saturating_add(1);
            }
            Arc::make_mut(&mut state.events).push_back(event.clone_event_arc());
            state.pressure.entries = state.pressure.entries.saturating_add(1);
            state.pressure.used_bytes = state.pressure.used_bytes.saturating_add(retained_bytes);
            state.pressure.peak_bytes = state.pressure.peak_bytes.max(state.pressure.used_bytes);
        }
        drop(state);
        let drains = Arc::clone(&self.drains.read());
        for drain in drains.iter() {
            drain.emit_shared(event);
        }
    }
}

fn default_max_bytes(capacity: usize) -> usize {
    capacity
        .saturating_mul(maximum_observation_retained_bytes())
        .clamp(1, MAX_IN_MEMORY_OBSERVATION_BYTES)
}

fn maximum_observation_retained_bytes() -> usize {
    OBSERVATION_FIXED_BYTES
        .saturating_add(MAX_OBSERVATION_NAME_BYTES)
        .saturating_add(MAX_OBSERVATION_TRACE_BYTES)
        .saturating_add(
            MAX_OBSERVATION_ATTRIBUTES.saturating_mul(
                ATTRIBUTE_FIXED_BYTES
                    .saturating_add(MAX_OBSERVATION_KEY_BYTES)
                    .saturating_add(MAX_OBSERVATION_VALUE_BYTES),
            ),
        )
}

fn observation_retained_bytes(event: &ObservationEvent) -> usize {
    OBSERVATION_FIXED_BYTES
        .saturating_add(event.name.capacity())
        .saturating_add(event.trace_id.as_ref().map_or(0, String::capacity))
        .saturating_add(event.attributes.iter().fold(0usize, |total, (key, value)| {
            total
                .saturating_add(ATTRIBUTE_FIXED_BYTES)
                .saturating_add(key.capacity())
                .saturating_add(value.capacity())
        }))
}

fn is_sensitive_key(key: &str) -> bool {
    ["secret", "password", "token", "credential", "private_key"]
        .iter()
        .any(|fragment| {
            key.as_bytes()
                .windows(fragment.len())
                .any(|candidate| candidate.eq_ignore_ascii_case(fragment.as_bytes()))
        })
}

#[cfg(test)]
#[path = "observation_tests.rs"]
mod tests;
