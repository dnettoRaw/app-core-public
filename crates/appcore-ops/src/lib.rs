// =============================================================================
//        #######
//     ###       ###     F: lib.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/05/29 20:47:35 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/24 16:07:49 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Observability contracts for runtime health, logging, and heartbeat signals.

#![deny(missing_docs)]

mod availability;
pub mod health;
pub mod heartbeat;
pub mod log;
/// Structured logging infrastructure shared by operational observers.
pub mod logging {
    pub use appcore_log::{
        ConfiguredLogger, ConsoleSink, FileArchiveConfig, FileSink, FileSinkConfig, LogConfigError,
        LogDispatcher, LogEvent, LogField, LogOutputMode, LogPolicy, LogSink, LogStats,
        LoggerConfig, PathAliases, RingBufferSink, SensitiveDntSink, SensitiveDntSinkConfig,
        Sensitivity, Severity, Verbosity, LOG_SIZE_16_MIB, LOG_SIZE_1_MIB, LOG_SIZE_2_MIB,
        LOG_SIZE_32_MIB, LOG_SIZE_4_MIB, LOG_SIZE_64_MIB, LOG_SIZE_8_MIB,
    };
}
pub mod metrics;
pub mod observation;
mod observation_file;
mod observation_flush;
mod observation_metrics;

pub use availability::{RuntimeAvailabilityReport, RuntimeAvailabilityState};
pub use health::{BasicHealthCheck, HealthCheck, HealthReport, HealthStatus};
pub use heartbeat::{Heartbeat, HeartbeatSource, StaticHeartbeatSource};
pub use log::{
    InMemoryLogPressure, InMemoryLogger, LogLevel, LogRecord, LogSnapshot, RuntimeLogger,
    StdoutLogger, MAX_IN_MEMORY_LOG_BYTES, MAX_IN_MEMORY_LOG_RECORDS, MAX_LOG_TARGET_BYTES,
};
pub use metrics::{
    InMemoryMetrics, MetricCounter, MetricRegistryPressure, MetricSnapshot, MAX_IN_MEMORY_METRICS,
    MAX_IN_MEMORY_METRIC_BYTES, MAX_METRIC_NAME_BYTES,
};
pub use observation::{
    InMemoryObservationPressure, InMemoryObservationSink, ObservationEvent, ObservationKind,
    ObservationSeverity, ObservationSink, ObservationSnapshot, SharedObservationEvent,
    MAX_IN_MEMORY_OBSERVATION_BYTES, MAX_IN_MEMORY_OBSERVATION_ITEMS, MAX_OBSERVATION_ATTRIBUTES,
    MAX_OBSERVATION_DRAINS, MAX_OBSERVATION_KEY_BYTES, MAX_OBSERVATION_NAME_BYTES,
    MAX_OBSERVATION_TRACE_BYTES, MAX_OBSERVATION_VALUE_BYTES,
};
pub use observation_file::{
    FileObservationSink, FileObservationSinkConfig, FileObservationSinkPressure,
    FileObservationSinkStats, FILE_OBSERVATION_FLUSH_TIMEOUT, MAX_FILE_OBSERVATION_QUEUE_BYTES,
    MAX_FILE_OBSERVATION_QUEUE_ITEMS, OBSERVATION_FILE_FORMAT_V1,
};
pub use observation_metrics::ObservationMetricsSink;
