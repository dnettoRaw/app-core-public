// =============================================================================
//        #######
//     ###       ###     F: log_tests.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/06/04 11:57:41 by dnettoRaw
//    ##   ## ##   ##    U: 2026/07/21 10:48:21 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================
// appcore-norm: test

use super::{InMemoryLogger, LogLevel, LogRecord, RuntimeLogger, StdoutLogger};

#[test]
fn log_record_basico() {
    let record = LogRecord {
        level: LogLevel::Info,
        target: "runtime.controller".to_string(),
        message: "dispatch accepted".to_string(),
        timestamp_ms: 42,
    };

    assert_eq!(record.level, LogLevel::Info);
    assert_eq!(record.target, "runtime.controller");
    assert_eq!(record.message, "dispatch accepted");
    assert_eq!(record.timestamp_ms, 42);
}

#[test]
fn in_memory_logger_captures_logs() {
    let logger = InMemoryLogger::new();

    logger.log(LogRecord {
        level: LogLevel::Warn,
        target: "runtime.lifecycle".to_string(),
        message: "restricted mode".to_string(),
        timestamp_ms: 77,
    });

    assert_eq!(logger.len(), 1);
    assert_eq!(logger.records()[0].target, "runtime.lifecycle");
}

#[test]
fn stdout_logger_can_log() {
    let logger = StdoutLogger::new();
    logger.log(LogRecord {
        level: LogLevel::Info,
        target: "runtime.server".to_string(),
        message: "boot ready".to_string(),
        timestamp_ms: 1,
    });
}

#[test]
fn in_memory_logger_redacts_credentials() {
    let logger = InMemoryLogger::new();
    logger.log(LogRecord {
        level: LogLevel::Error,
        target: "runtime.security".to_string(),
        message: "token=top-secret password=also-secret".to_string(),
        timestamp_ms: 1,
    });

    let records = logger.records();
    assert_eq!(records[0].message, "token=[REDACTED] password=[REDACTED]");
}
