// =============================================================================
//        #######
//     ###       ###     F: dispatcher.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Dispatcher ownership and bounded failure accounting.

use crate::{LogClock, LogEvent, LogPolicy, LogSink, Sensitivity, Severity};
use std::borrow::Cow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Observable dispatcher counters; sink failures never recursively log.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LogStats {
    /// Events filtered before formatting.
    pub filtered: u64,
    /// Sink deliveries that failed.
    pub sink_failures: u64,
    /// Events dropped by bounded sinks.
    pub dropped: u64,
    /// Events rejected because a public event limit was exceeded.
    pub invalid: u64,
}

/// Failure counter for one configured sink label.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SinkStats {
    /// Stable sink implementation label.
    pub name: &'static str,
    /// Controlled delivery failures for that sink.
    pub failures: u64,
}

struct SinkSlot {
    sink: Arc<dyn LogSink>,
    failures: AtomicU64,
}

/// Thread-safe fan-out dispatcher; ordinary sinks receive only sanitized events.
pub struct LogDispatcher {
    policy: LogPolicy,
    sinks: Vec<SinkSlot>,
    filtered: AtomicU64,
    sink_failures: AtomicU64,
    dropped: AtomicU64,
    invalid: AtomicU64,
}

impl LogDispatcher {
    /// Creates a dispatcher with explicit filtering and sinks.
    pub fn new(policy: LogPolicy, sinks: Vec<Arc<dyn LogSink>>) -> Self {
        Self {
            policy,
            sinks: sinks
                .into_iter()
                .map(|sink| SinkSlot {
                    sink,
                    failures: AtomicU64::new(0),
                })
                .collect(),
            filtered: AtomicU64::new(0),
            sink_failures: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
            invalid: AtomicU64::new(0),
        }
    }
    /// Emits once. Critical failure accounting is retained even when a sink fails.
    pub fn emit(&self, event: LogEvent) {
        if self.sinks.is_empty() {
            return;
        }
        if event.validate().is_err() {
            self.invalid.fetch_add(1, Ordering::Relaxed);
            return;
        }
        if !self.policy.allows(&event) {
            self.filtered.fetch_add(1, Ordering::Relaxed);
            return;
        }
        let severity = event.severity;
        let sanitized = self.policy.sanitize_owned(event);
        for slot in &self.sinks {
            if self.policy.sensitivity == Sensitivity::Sensitive && !slot.sink.accepts_sensitive() {
                self.sink_failures.fetch_add(1, Ordering::Relaxed);
                slot.failures.fetch_add(1, Ordering::Relaxed);
                continue;
            }
            if slot.sink.emit(&sanitized).is_err() {
                self.sink_failures.fetch_add(1, Ordering::Relaxed);
                slot.failures.fetch_add(1, Ordering::Relaxed);
                if severity < Severity::Error {
                    self.dropped.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    }

    /// Creates a fluent event builder for a stable component and clock value.
    pub fn event<'a>(
        &'a self,
        timestamp_ms: u64,
        component: impl Into<Cow<'a, str>>,
    ) -> LogBuilder<'a> {
        LogBuilder {
            dispatcher: self,
            timestamp_ms,
            component: component.into(),
            verbosity: crate::Verbosity::V4,
        }
    }

    /// Creates a fluent event builder using an injected clock.
    pub fn event_now<'a>(
        &'a self,
        clock: &dyn LogClock,
        component: impl Into<Cow<'a, str>>,
    ) -> LogBuilder<'a> {
        self.event(clock.now_ms(), component)
    }

    /// Reports whether a component and verbosity would reach at least one sink.
    pub fn enabled(&self, component: &str, verbosity: crate::Verbosity) -> bool {
        !self.sinks.is_empty() && self.policy.allows_component(component, verbosity)
    }
    /// Returns counters without taking global locks.
    pub fn stats(&self) -> LogStats {
        LogStats {
            filtered: self.filtered.load(Ordering::Relaxed),
            sink_failures: self.sink_failures.load(Ordering::Relaxed),
            dropped: self.dropped.load(Ordering::Relaxed),
            invalid: self.invalid.load(Ordering::Relaxed),
        }
    }

    /// Returns bounded failure counters for each configured sink.
    pub fn sink_stats(&self) -> Vec<SinkStats> {
        self.sinks
            .iter()
            .map(|slot| SinkStats {
                name: slot.sink.name(),
                failures: slot.failures.load(Ordering::Relaxed),
            })
            .collect()
    }
}

/// Fluent, component-scoped log emitter with an immutable per-event override.
#[derive(Clone)]
pub struct LogBuilder<'a> {
    dispatcher: &'a LogDispatcher,
    timestamp_ms: u64,
    component: Cow<'a, str>,
    verbosity: crate::Verbosity,
}

impl<'a> LogBuilder<'a> {
    /// Replaces the stable component used for filtering and rendering.
    #[must_use]
    pub fn component(&self, component: impl Into<Cow<'a, str>>) -> Self {
        Self {
            component: component.into(),
            ..self.clone()
        }
    }

    /// Sets a V1–V9 threshold for this event; invalid values retain V4.
    ///
    /// Use [`Self::try_verbosity`] when invalid configuration must be reported.
    #[must_use]
    pub fn verbosity(&self, verbosity: u8) -> Self {
        Self {
            verbosity: crate::Verbosity::new(verbosity).unwrap_or(crate::Verbosity::V4),
            ..self.clone()
        }
    }

    /// Sets a V1–V9 threshold and returns invalid configuration explicitly.
    pub fn try_verbosity(&self, verbosity: u8) -> Result<Self, crate::VerbosityError> {
        let verbosity = crate::Verbosity::new(verbosity).ok_or(crate::VerbosityError)?;
        Ok(Self {
            verbosity,
            ..self.clone()
        })
    }

    /// Emits a trace event.
    pub fn trace(&self, message: impl Into<String>) {
        self.emit(Severity::Trace, message);
    }

    /// Emits a debug event.
    pub fn debug(&self, message: impl Into<String>) {
        self.emit(Severity::Debug, message);
    }

    /// Emits an informational event.
    pub fn info(&self, message: impl Into<String>) {
        self.emit(Severity::Info, message);
    }

    /// Emits a warning event.
    pub fn warn(&self, message: impl Into<String>) {
        self.emit(Severity::Warn, message);
    }

    /// Emits an error event.
    pub fn error(&self, message: impl Into<String>) {
        self.emit(Severity::Error, message);
    }

    /// Emits a critical event.
    pub fn critical(&self, message: impl Into<String>) {
        self.emit(Severity::Critical, message);
    }

    fn emit(&self, severity: Severity, message: impl Into<String>) {
        if self.dispatcher.enabled(&self.component, self.verbosity) {
            self.dispatcher.emit(LogEvent::new(
                self.timestamp_ms,
                severity,
                self.verbosity,
                self.component.to_string(),
                message.into(),
            ));
        }
    }
}
