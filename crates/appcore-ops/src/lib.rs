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
pub mod metrics;
pub mod observation;
mod observation_file;
mod observation_metrics;

pub use availability::{RuntimeAvailabilityReport, RuntimeAvailabilityState};
pub use health::{BasicHealthCheck, HealthCheck, HealthReport, HealthStatus};
pub use heartbeat::{Heartbeat, HeartbeatSource, StaticHeartbeatSource};
pub use log::{InMemoryLogger, LogLevel, LogRecord, RuntimeLogger, StdoutLogger};
pub use metrics::{InMemoryMetrics, MetricCounter};
pub use observation::{
    InMemoryObservationSink, ObservationEvent, ObservationKind, ObservationSeverity,
    ObservationSink, MAX_OBSERVATION_ATTRIBUTES, MAX_OBSERVATION_KEY_BYTES,
    MAX_OBSERVATION_NAME_BYTES, MAX_OBSERVATION_TRACE_BYTES, MAX_OBSERVATION_VALUE_BYTES,
};
pub use observation_file::{
    FileObservationSink, FileObservationSinkConfig, FileObservationSinkStats,
    OBSERVATION_FILE_FORMAT_V1,
};
pub use observation_metrics::ObservationMetricsSink;
