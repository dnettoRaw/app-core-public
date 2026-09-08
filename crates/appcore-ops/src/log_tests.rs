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

use super::{
    log_record_retained_bytes, InMemoryLogger, LogLevel, LogRecord, RuntimeLogger, StdoutLogger,
    MAX_IN_MEMORY_LOG_BYTES, MAX_IN_MEMORY_LOG_RECORDS,
};

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

fn record(message: &str, timestamp_ms: u64) -> LogRecord {
    LogRecord {
        level: LogLevel::Info,
        target: "runtime.test".to_string(),
        message: message.to_string(),
        timestamp_ms,
    }
}

#[test]
fn logger_enforces_count_and_keeps_shared_snapshots_stable() {
    let logger = InMemoryLogger::with_limits(1, MAX_IN_MEMORY_LOG_BYTES);
    logger.log(record("first", 1));
    let stable = logger.shared_records();
    logger.log(record("other", 2));

    assert_eq!(stable.iter().next().unwrap().message, "first");
    assert_eq!(
        logger.shared_records().iter().next().unwrap().message,
        "other"
    );
    assert_eq!(logger.pressure().entries, 1);
    assert_eq!(logger.pressure().evictions, 1);
}

#[test]
fn logger_enforces_byte_budget_and_reports_oversized_records() {
    let prepared = record("fits", 1);
    let one_record = log_record_retained_bytes(&prepared);
    let logger = InMemoryLogger::with_limits(2, one_record);
    logger.log(prepared);
    logger.log(record("too-large", 2));

    assert_eq!(logger.len(), 1);
    assert_eq!(logger.pressure().used_bytes, one_record);
    assert_eq!(logger.pressure().oversized_rejections, 1);
}

#[test]
fn logger_clamps_untrusted_limits_without_preallocation() {
    let logger = InMemoryLogger::with_limits(usize::MAX, usize::MAX);
    let pressure = logger.pressure();
    assert_eq!(pressure.max_entries, MAX_IN_MEMORY_LOG_RECORDS);
    assert_eq!(pressure.max_bytes, MAX_IN_MEMORY_LOG_BYTES);
    assert!(logger.shared_records().is_empty());
}
