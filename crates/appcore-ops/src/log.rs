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
use std::collections::VecDeque;
use std::io::Write;
use std::sync::Arc;

/// Maximum records retained by one in-memory logger.
pub const MAX_IN_MEMORY_LOG_RECORDS: usize = 4_096;
/// Absolute aggregate retained-byte ceiling for one in-memory logger.
pub const MAX_IN_MEMORY_LOG_BYTES: usize = 8 * 1024 * 1024;
/// Maximum UTF-8 bytes retained in one log target.
pub const MAX_LOG_TARGET_BYTES: usize = 128;
const LOG_RECORD_FIXED_BYTES: usize = std::mem::size_of::<LogRecord>()
    + std::mem::size_of::<Arc<LogRecord>>()
    + std::mem::size_of::<usize>() * 2;

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

/// Immutable shared view of retained log records, ordered oldest to newest.
#[derive(Debug, Clone, Default)]
pub struct LogSnapshot {
    records: Arc<VecDeque<Arc<LogRecord>>>,
}

impl LogSnapshot {
    /// Returns the number of retained records.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Reports whether no records are retained.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Iterates over retained records without cloning them.
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &LogRecord> {
        self.records.iter().map(AsRef::as_ref)
    }

    /// Iterates over at most the newest `limit` records in chronological order.
    pub fn recent(&self, limit: usize) -> impl Iterator<Item = &LogRecord> {
        self.records
            .iter()
            .skip(self.records.len().saturating_sub(limit))
            .map(AsRef::as_ref)
    }
}

/// Point-in-time count and retained-memory pressure for an in-memory logger.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InMemoryLogPressure {
    /// Current retained record count.
    pub entries: usize,
    /// Configured record ceiling.
    pub max_entries: usize,
    /// Estimated bytes retained by records and owned text.
    pub used_bytes: usize,
    /// Highest estimated retained byte count observed since creation.
    pub peak_bytes: usize,
    /// Configured aggregate retained-byte ceiling.
    pub max_bytes: usize,
    /// Oldest records discarded to enforce count or byte limits.
    pub evictions: u64,
    /// Records not retained because one record exceeded the byte ceiling.
    pub oversized_rejections: u64,
}

#[derive(Debug)]
struct LogState {
    records: Arc<VecDeque<Arc<LogRecord>>>,
    pressure: InMemoryLogPressure,
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
        let _ = writeln!(
            std::io::stdout().lock(),
            "[{:?}] {} {} {}",
            record.level,
            target,
            message,
            record.timestamp_ms
        );
    }
}

/// In-memory logger for deterministic tests.
#[derive(Debug)]
pub struct InMemoryLogger {
    state: Mutex<LogState>,
}

impl InMemoryLogger {
    /// Creates an empty in-memory logger.
    pub fn new() -> Self {
        Self::with_limits(MAX_IN_MEMORY_LOG_RECORDS, MAX_IN_MEMORY_LOG_BYTES)
    }

    /// Creates a logger under explicit limits clamped to public safety ceilings.
    pub fn with_limits(max_entries: usize, max_bytes: usize) -> Self {
        let max_entries = max_entries.clamp(1, MAX_IN_MEMORY_LOG_RECORDS);
        let max_bytes = max_bytes.clamp(1, MAX_IN_MEMORY_LOG_BYTES);
        Self {
            state: Mutex::new(LogState {
                records: Arc::new(VecDeque::new()),
                pressure: InMemoryLogPressure {
                    max_entries,
                    max_bytes,
                    ..InMemoryLogPressure::default()
                },
            }),
        }
    }

    /// Returns the number of retained records.
    pub fn len(&self) -> usize {
        self.state.lock().pressure.entries
    }

    /// Reports whether no records are retained.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a point-in-time copy of retained records.
    pub fn records(&self) -> Vec<LogRecord> {
        self.shared_records().iter().cloned().collect()
    }

    /// Returns an immutable snapshot without cloning retained records.
    pub fn shared_records(&self) -> LogSnapshot {
        LogSnapshot {
            records: Arc::clone(&self.state.lock().records),
        }
    }

    /// Returns current count and retained-memory pressure.
    pub fn pressure(&self) -> InMemoryLogPressure {
        self.state.lock().pressure
    }
}

impl Default for InMemoryLogger {
    fn default() -> Self {
        Self::new()
    }
}

impl RuntimeLogger for InMemoryLogger {
    fn log(&self, record: LogRecord) {
        let mut record = record;
        record.message = appcore_core::redact_text(&record.message);
        record.target = appcore_core::redact_text_with_limit(&record.target, MAX_LOG_TARGET_BYTES);
        record.message.shrink_to_fit();
        record.target.shrink_to_fit();
        let record = Arc::new(record);
        let retained_bytes = log_record_retained_bytes(&record);
        let mut state = self.state.lock();
        if retained_bytes > state.pressure.max_bytes {
            state.pressure.oversized_rejections =
                state.pressure.oversized_rejections.saturating_add(1);
            return;
        }
        while state.pressure.entries >= state.pressure.max_entries
            || state.pressure.used_bytes.saturating_add(retained_bytes) > state.pressure.max_bytes
        {
            let Some(removed) = Arc::make_mut(&mut state.records).pop_front() else {
                break;
            };
            state.pressure.entries = state.pressure.entries.saturating_sub(1);
            state.pressure.used_bytes = state
                .pressure
                .used_bytes
                .saturating_sub(log_record_retained_bytes(&removed));
            state.pressure.evictions = state.pressure.evictions.saturating_add(1);
        }
        Arc::make_mut(&mut state.records).push_back(record);
        state.pressure.entries = state.pressure.entries.saturating_add(1);
        state.pressure.used_bytes = state.pressure.used_bytes.saturating_add(retained_bytes);
        state.pressure.peak_bytes = state.pressure.peak_bytes.max(state.pressure.used_bytes);
    }
}

fn log_record_retained_bytes(record: &LogRecord) -> usize {
    LOG_RECORD_FIXED_BYTES
        .saturating_add(record.target.capacity())
        .saturating_add(record.message.capacity())
}

#[cfg(test)]
#[path = "log_tests.rs"]
mod tests;
