// =============================================================================
//        #######
//     ###       ###     F: config.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! High-level destination selection without global logging state.

use crate::{
    ConsoleSink, FileSink, FileSinkConfig, LogDispatcher, LogError, LogPolicy, LogSink,
    RingBufferSink,
};
use std::sync::Arc;

/// Explicit destinations for ordinary operational logs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogOutputMode {
    /// Perform no logging work after the cheap enabled check.
    Disabled,
    /// Write human-readable output to the terminal.
    Terminal,
    /// Write bounded structured JSONL files.
    File,
    /// Write to both terminal and bounded JSONL files.
    TerminalAndFile,
    /// Retain a bounded sanitized ring and write it only on explicit crash dump.
    CrashOnly,
}

/// Invalid high-level logger configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogConfigError {
    /// The selected mode requires a file configuration.
    MissingFile,
    /// A sink rejected its bounded configuration.
    Sink(LogError),
}

impl From<LogError> for LogConfigError {
    fn from(error: LogError) -> Self {
        Self::Sink(error)
    }
}

/// Complete explicit logger configuration.
#[derive(Debug, Clone)]
pub struct LoggerConfig {
    /// Filtering and sanitization policy.
    pub policy: LogPolicy,
    /// Terminal, file, both, disabled or crash-only behavior.
    pub output: LogOutputMode,
    /// File configuration used by file output or an explicit crash dump.
    pub file: Option<FileSinkConfig>,
    /// Maximum events retained by crash-only mode.
    pub crash_events: usize,
    /// Maximum estimated bytes retained by crash-only mode.
    pub crash_bytes: usize,
}

impl Default for LoggerConfig {
    fn default() -> Self {
        Self {
            policy: LogPolicy::default(),
            output: LogOutputMode::Terminal,
            file: None,
            crash_events: 256,
            crash_bytes: 1024 * 1024,
        }
    }
}

impl LoggerConfig {
    /// Builds a logger with no hidden global state or background thread.
    pub fn build(self) -> Result<ConfiguredLogger, LogConfigError> {
        let Self {
            policy,
            output,
            file,
            crash_events,
            crash_bytes,
        } = self;
        let mut sinks = Vec::<Arc<dyn LogSink>>::with_capacity(2);
        let mut crash_ring = None;
        let mut crash_sink = None;

        if matches!(
            output,
            LogOutputMode::Terminal | LogOutputMode::TerminalAndFile
        ) {
            sinks.push(Arc::new(ConsoleSink::new()));
        }
        if matches!(output, LogOutputMode::File | LogOutputMode::TerminalAndFile) {
            let file = file.ok_or(LogConfigError::MissingFile)?;
            sinks.push(Arc::new(FileSink::new(file)?));
        } else if output == LogOutputMode::CrashOnly {
            let file = file.ok_or(LogConfigError::MissingFile)?;
            let ring = Arc::new(RingBufferSink::new(crash_events, crash_bytes)?);
            sinks.push(ring.clone());
            crash_ring = Some(ring);
            crash_sink = Some(FileSink::new(file)?);
        }

        Ok(ConfiguredLogger {
            dispatcher: LogDispatcher::new(policy, sinks),
            crash_ring,
            crash_sink,
        })
    }
}

/// Logger assembled from [`LoggerConfig`], including optional crash retention.
pub struct ConfiguredLogger {
    dispatcher: LogDispatcher,
    crash_ring: Option<Arc<RingBufferSink>>,
    crash_sink: Option<FileSink>,
}

impl ConfiguredLogger {
    /// Returns the dispatcher used by application and Runtime components.
    pub fn dispatcher(&self) -> &LogDispatcher {
        &self.dispatcher
    }

    /// Writes the sanitized crash ring to its configured bounded file.
    pub fn dump_crash(&self) -> Result<usize, LogConfigError> {
        let Some(ring) = &self.crash_ring else {
            return Ok(0);
        };
        let sink = self
            .crash_sink
            .as_ref()
            .ok_or(LogConfigError::MissingFile)?;
        let events = ring.snapshot();
        for event in &events {
            sink.emit(event)?;
        }
        Ok(events.len())
    }
}
