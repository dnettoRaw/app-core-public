// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Safe structured operational logging for `AppCore`.
//!
//! Events are filtered before formatting, sanitized before every ordinary sink,
//! and retained only under explicit count and byte limits. Severity describes
//! impact; [`Verbosity`] describes detail and is independent of sensitivity.
//! `Sensitive` policy output is accepted only by [`SensitiveDntSink`], which
//! writes an authenticated encrypted `DNT` envelope and never falls back to text.

#![deny(missing_docs)]

mod async_sink;
mod clock;
mod config;
mod dispatcher;
mod event;
mod json_line;
mod policy;
mod sink;
mod sizes;

pub use async_sink::{
    AsyncSink, AsyncSinkConfig, AsyncSinkStats, MAX_ASYNC_LOG_BYTES, MAX_ASYNC_LOG_EVENTS,
};
pub use clock::{FixedLogClock, LogClock, SystemLogClock};
pub use config::{ConfiguredLogger, LogConfigError, LogOutputMode, LoggerConfig};
pub use dispatcher::{LogBuilder, LogDispatcher, LogStats, SinkStats};
pub use event::{
    LogEvent, LogEventError, LogField, Severity, Verbosity, VerbosityError, MAX_LOG_FIELDS,
    MAX_LOG_FIELD_KEY_BYTES, MAX_LOG_FIELD_VALUE_BYTES, MAX_LOG_TEXT_BYTES,
};
pub use policy::{LogPolicy, PathAliases, Sensitivity};
pub use sink::{
    ConsoleSink, FileArchiveConfig, FileSink, FileSinkConfig, LogError, LogSink, RingBufferSink,
    SensitiveDntSink, SensitiveDntSinkConfig,
};
pub use sizes::{
    LOG_SIZE_16_MIB, LOG_SIZE_1_MIB, LOG_SIZE_2_MIB, LOG_SIZE_32_MIB, LOG_SIZE_4_MIB,
    LOG_SIZE_64_MIB, LOG_SIZE_8_MIB,
};

#[cfg(test)]
mod tests;
