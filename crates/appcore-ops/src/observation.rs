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
        let key = redact_text_with_limit(&key.into(), MAX_OBSERVATION_KEY_BYTES);
        if self.attributes.len() >= MAX_OBSERVATION_ATTRIBUTES
            && !self.attributes.contains_key(&key)
        {
            return self;
        }
        let value = if is_sensitive_key(&key) {
            "[REDACTED]".to_string()
        } else {
            redact_text_with_limit(&value.into(), MAX_OBSERVATION_VALUE_BYTES)
        };
        self.attributes.insert(key, value);
        self
    }

    pub(crate) fn redacted(mut self) -> Self {
        self.name = redact_text_with_limit(&self.name, MAX_OBSERVATION_NAME_BYTES);
        self.trace_id = self
            .trace_id
            .map(|value| redact_text_with_limit(&value, MAX_OBSERVATION_TRACE_BYTES));
        while self.attributes.len() > MAX_OBSERVATION_ATTRIBUTES {
            let Some(key) = self.attributes.keys().next_back().cloned() else {
                break;
            };
            self.attributes.remove(&key);
        }
        for (key, value) in &mut self.attributes {
            *value = if is_sensitive_key(key) {
                "[REDACTED]".to_string()
            } else {
                redact_text_with_limit(value, MAX_OBSERVATION_VALUE_BYTES)
            };
        }
        self
    }
}

/// Destination for generic observation events.
pub trait ObservationSink: Send + Sync {
    /// Emits one event without blocking on external tooling.
    fn emit(&self, event: ObservationEvent);
}

/// Bounded in-memory sink used by diagnostics and embedded runtimes.
#[derive(Clone)]
pub struct InMemoryObservationSink {
    capacity: usize,
    events: Arc<Mutex<VecDeque<ObservationEvent>>>,
    drains: Arc<RwLock<Vec<Arc<dyn ObservationSink>>>>,
}

impl std::fmt::Debug for InMemoryObservationSink {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InMemoryObservationSink")
            .field("capacity", &self.capacity)
            .field("event_count", &self.len())
            .field("drain_count", &self.drains.read().len())
            .finish()
    }
}

impl InMemoryObservationSink {
    /// Creates a sink that retains the newest `capacity` events.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            events: Arc::new(Mutex::new(VecDeque::with_capacity(capacity.max(1)))),
            drains: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Adds an operational drain that receives future redacted events.
    pub fn add_drain(&self, drain: Arc<dyn ObservationSink>) {
        self.drains.write().push(drain);
    }

    /// Returns the number of attached operational drains.
    pub fn drain_count(&self) -> usize {
        self.drains.read().len()
    }

    /// Returns a stable snapshot from oldest to newest.
    pub fn snapshot(&self) -> Vec<ObservationEvent> {
        self.events.lock().iter().cloned().collect()
    }

    /// Returns the number of retained events.
    pub fn len(&self) -> usize {
        self.events.lock().len()
    }

    /// Reports whether no events are retained.
    pub fn is_empty(&self) -> bool {
        self.events.lock().is_empty()
    }
}

impl Default for InMemoryObservationSink {
    fn default() -> Self {
        Self::new(1_024)
    }
}

impl ObservationSink for InMemoryObservationSink {
    fn emit(&self, event: ObservationEvent) {
        let event = event.redacted();
        let mut events = self.events.lock();
        if events.len() == self.capacity {
            let _ = events.pop_front();
        }
        events.push_back(event.clone());
        drop(events);
        let drains = self.drains.read().clone();
        for drain in drains {
            drain.emit(event.clone());
        }
    }
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.to_ascii_lowercase();
    ["secret", "password", "token", "credential", "private_key"]
        .iter()
        .any(|fragment| normalized.contains(fragment))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_sink_discards_oldest_event() {
        let sink = InMemoryObservationSink::new(2);
        for index in 0..3 {
            sink.emit(ObservationEvent::new(
                ObservationKind::Lifecycle,
                ObservationSeverity::Info,
                format!("runtime.event.{index}"),
                index,
            ));
        }
        let snapshot = sink.snapshot();
        assert_eq!(snapshot.len(), 2);
        assert_eq!(snapshot[0].name, "runtime.event.1");
    }

    #[test]
    fn sink_redacts_sensitive_attributes() {
        let sink = InMemoryObservationSink::new(2);
        sink.emit(
            ObservationEvent::new(
                ObservationKind::Security,
                ObservationSeverity::Warning,
                "security.rejected",
                1,
            )
            .with_attribute("access_token", "raw-secret"),
        );
        assert_eq!(sink.snapshot()[0].attributes["access_token"], "[REDACTED]");
    }

    #[test]
    fn sink_bounds_names_values_and_attribute_count() {
        let mut event = ObservationEvent::new(
            ObservationKind::Diagnostic,
            ObservationSeverity::Info,
            "n".repeat(1_000),
            1,
        );
        for index in 0..100 {
            event = event.with_attribute(format!("key-{index}"), "v".repeat(2_000));
        }
        let sink = InMemoryObservationSink::new(1);
        sink.emit(event);
        let event = &sink.snapshot()[0];

        assert!(event.name.len() <= MAX_OBSERVATION_NAME_BYTES);
        assert_eq!(event.attributes.len(), MAX_OBSERVATION_ATTRIBUTES);
        assert!(event
            .attributes
            .values()
            .all(|value| value.len() <= MAX_OBSERVATION_VALUE_BYTES));
    }
}
