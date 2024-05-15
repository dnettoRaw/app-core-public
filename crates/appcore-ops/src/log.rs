// =============================================================================
//        #######
//     ###       ###     F: log.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/31 13:38:42 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/23 23:50:45 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Logging contracts for runtime observability.

use parking_lot::Mutex;

/// Structured log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    /// Fine-grained diagnostic event.
    Trace,
    /// Developer diagnostic event.
    Debug,
    /// Normal operational event.
    Info,
    /// Recoverable or degraded condition.
    Warn,
    /// Failed operation.
    Error,
}

/// One runtime log record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    /// Record severity.
    pub level: LogLevel,
    /// Stable Runtime subsystem target.
    pub target: String,
    /// Non-sensitive message subject to redaction.
    pub message: String,
    /// Timestamp in Unix milliseconds.
    pub timestamp_ms: u64,
}

/// Contract for runtime log sinks.
pub trait RuntimeLogger: Send + Sync {
    /// Emits one structured Runtime log record.
    fn log(&self, record: LogRecord);
}

/// Minimal stdout logger for local runtime operation.
#[derive(Debug, Default, Clone, Copy)]
pub struct StdoutLogger;

impl StdoutLogger {
    /// Creates a stdout logger.
    pub fn new() -> Self {
        Self
    }
}

impl RuntimeLogger for StdoutLogger {
    fn log(&self, record: LogRecord) {
        let message = appcore_core::redact_text(&record.message);
        let target = appcore_core::redact_text_with_limit(&record.target, 128);
        println!(
            "[{:?}] {} {} {}",
            record.level, target, message, record.timestamp_ms
        );
    }
}

/// In-memory logger for deterministic tests.
#[derive(Debug, Default)]
pub struct InMemoryLogger {
    records: Mutex<Vec<LogRecord>>,
}

impl InMemoryLogger {
    /// Creates an empty in-memory logger.
    pub fn new() -> Self {
        Self {
            records: Mutex::new(Vec::new()),
        }
    }

    /// Returns the number of retained records.
    pub fn len(&self) -> usize {
        self.records.lock().len()
    }

    /// Reports whether no records are retained.
    pub fn is_empty(&self) -> bool {
        self.records.lock().is_empty()
    }

    /// Returns a point-in-time copy of retained records.
    pub fn records(&self) -> Vec<LogRecord> {
        self.records.lock().clone()
    }
}

impl RuntimeLogger for InMemoryLogger {
    fn log(&self, record: LogRecord) {
        let mut record = record;
        record.message = appcore_core::redact_text(&record.message);
        record.target = appcore_core::redact_text_with_limit(&record.target, 128);
        self.records.lock().push(record);
    }
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod tests;
