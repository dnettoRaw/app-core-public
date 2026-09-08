// =============================================================================
//        #######
//     ###       ###     F: integration.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: unknown by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

// appcore-norm: test

use appcore_log::{
    AsyncSink, AsyncSinkConfig, FileArchiveConfig, FileSink, FileSinkConfig, LogDispatcher,
    LogEvent, LogOutputMode, LogPolicy, LogSink, LoggerConfig, RingBufferSink, Severity, Verbosity,
};
use std::sync::Arc;

#[test]
fn public_async_sink_flushes_and_shuts_down() {
    let ring = Arc::new(RingBufferSink::new(2, 4096).unwrap());
    let asynchronous = Arc::new(
        AsyncSink::new(
            AsyncSinkConfig {
                max_events: 2,
                max_bytes: 4096,
            },
            ring.clone(),
        )
        .unwrap(),
    );
    let dispatcher = LogDispatcher::new(LogPolicy::default(), vec![asynchronous.clone()]);

    dispatcher.emit(LogEvent::new(
        1,
        Severity::Info,
        Verbosity::V4,
        "application",
        "queued event",
    ));
    asynchronous.flush().unwrap();

    assert_eq!(ring.snapshot().len(), 1);
    assert_eq!(asynchronous.stats().delivered, 1);
    asynchronous.shutdown().unwrap();
}

#[test]
fn dispatcher_sanitizes_before_file_and_ring_sinks() {
    let path = std::env::temp_dir().join(format!("appcore-log-file-{}.jsonl", std::process::id()));
    let file = Arc::new(
        FileSink::new(FileSinkConfig {
            path: path.clone(),
            max_bytes: 4096,
            sync_each_write: false,
            retention: 1,
            archive: None,
        })
        .unwrap(),
    );
    let ring = Arc::new(RingBufferSink::new(2, 4096).unwrap());
    let dispatcher = LogDispatcher::new(LogPolicy::default(), vec![file, ring.clone()]);
    dispatcher.emit(LogEvent::new(
        1,
        Severity::Info,
        Verbosity::V4,
        "application",
        "password=not-shared",
    ));
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(text.contains("<REDACTED>"));
    assert!(!text.contains("not-shared"));
    assert_eq!(ring.snapshot().len(), 1);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn file_sink_rotates_without_exceeding_retention() {
    let path =
        std::env::temp_dir().join(format!("appcore-log-rotate-{}.jsonl", std::process::id()));
    let sink = FileSink::new(FileSinkConfig {
        path: path.clone(),
        max_bytes: 512,
        sync_each_write: false,
        retention: 1,
        archive: None,
    })
    .unwrap();
    for timestamp_ms in 0..6 {
        sink.emit(&LogEvent::new(
            timestamp_ms,
            Severity::Info,
            Verbosity::V4,
            "application",
            "a bounded record that rotates",
        ))
        .unwrap();
    }
    assert!(path.is_file());
    assert!(path.with_extension("jsonl.1").is_file());
    assert!(!path.with_extension("jsonl.2").exists());
    std::fs::remove_file(&path).unwrap();
    std::fs::remove_file(path.with_extension("jsonl.1")).unwrap();
}

#[cfg(unix)]
#[test]
fn file_sink_rejects_a_symlink_destination() {
    use std::os::unix::fs::symlink;

    let target =
        std::env::temp_dir().join(format!("appcore-log-target-{}.jsonl", std::process::id()));
    let path = std::env::temp_dir().join(format!("appcore-log-link-{}.jsonl", std::process::id()));
    std::fs::write(&target, "protected").unwrap();
    symlink(&target, &path).unwrap();
    let sink = FileSink::new(FileSinkConfig {
        path: path.clone(),
        max_bytes: 4096,
        sync_each_write: false,
        retention: 1,
        archive: None,
    })
    .unwrap();
    let result = sink.emit(&LogEvent::new(
        18,
        Severity::Info,
        Verbosity::V4,
        "application",
        "must not follow a link",
    ));
    assert!(result.is_err());
    assert_eq!(std::fs::read_to_string(&target).unwrap(), "protected");
    std::fs::remove_file(path).unwrap();
    std::fs::remove_file(target).unwrap();
}

#[test]
fn crash_only_keeps_the_file_absent_until_explicit_dump() {
    let path =
        std::env::temp_dir().join(format!("appcore-crash-only-{}.jsonl", std::process::id()));
    let logger = LoggerConfig {
        output: LogOutputMode::CrashOnly,
        file: Some(FileSinkConfig {
            path: path.clone(),
            max_bytes: 4096,
            sync_each_write: true,
            retention: 1,
            archive: None,
        }),
        crash_events: 4,
        crash_bytes: 4096,
        ..LoggerConfig::default()
    }
    .build()
    .unwrap();
    logger
        .dispatcher()
        .event(20, "application")
        .error("controlled crash");
    assert!(!path.exists());
    assert_eq!(logger.dump_crash().unwrap(), 1);
    assert!(path.is_file());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn combined_mode_writes_file_and_exposes_both_destinations() {
    let path = std::env::temp_dir().join(format!(
        "appcore-terminal-and-file-{}.jsonl",
        std::process::id()
    ));
    let logger = LoggerConfig {
        output: LogOutputMode::TerminalAndFile,
        file: Some(FileSinkConfig {
            path: path.clone(),
            max_bytes: 4096,
            sync_each_write: false,
            retention: 0,
            archive: None,
        }),
        ..LoggerConfig::default()
    }
    .build()
    .unwrap();

    logger
        .dispatcher()
        .event(21, "application")
        .info("combined output");

    let names = logger
        .dispatcher()
        .sink_stats()
        .into_iter()
        .map(|stats| stats.name)
        .collect::<Vec<_>>();
    assert_eq!(names, ["console", "file"]);
    assert!(path.is_file());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn zero_active_rotations_still_archive_the_previous_file() {
    let base =
        std::env::temp_dir().join(format!("appcore-log-zero-retention-{}", std::process::id()));
    let path = base.join("chosen-name.jsonl");
    let archive = base.join("history");
    std::fs::create_dir_all(&base).unwrap();
    let sink = FileSink::new(FileSinkConfig {
        path: path.clone(),
        max_bytes: 512,
        sync_each_write: false,
        retention: 0,
        archive: Some(FileArchiveConfig {
            directory: archive.clone(),
            max_files: 2,
        }),
    })
    .unwrap();

    for timestamp_ms in 0..8 {
        sink.emit(&LogEvent::new(
            timestamp_ms,
            Severity::Info,
            Verbosity::V4,
            "application",
            "this record forces a bounded rotation",
        ))
        .unwrap();
    }

    assert!(path.is_file());
    assert!(archive.join("1970").join("01").is_dir());
    assert!(!path.with_extension("jsonl.1").exists());
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn old_rotations_move_into_a_bounded_year_month_archive() {
    let base = std::env::temp_dir().join(format!("appcore-log-archive-{}", std::process::id()));
    let path = base.join("runtime.jsonl");
    let archive = base.join("archive");
    std::fs::create_dir_all(&base).unwrap();
    let sink = FileSink::new(FileSinkConfig {
        path: path.clone(),
        max_bytes: 512,
        sync_each_write: false,
        retention: 1,
        archive: Some(FileArchiveConfig {
            directory: archive.clone(),
            max_files: 2,
        }),
    })
    .unwrap();
    for timestamp_ms in 0..12 {
        sink.emit(&LogEvent::new(
            timestamp_ms,
            Severity::Info,
            Verbosity::V4,
            "application",
            "a bounded record that enters the archive",
        ))
        .unwrap();
    }
    let month = archive.join("1970").join("01");
    let archived = std::fs::read_dir(month).unwrap().count();
    assert!((1..=2).contains(&archived));
    std::fs::remove_dir_all(base).unwrap();
}
