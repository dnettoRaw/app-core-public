// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: working-tree by dnettoRaw
//    ##   ## ##   ##    U: working-tree by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Measures filtered, sanitized, in-memory, and bounded file logging paths.

mod contention;

use appcore_log::{
    FileArchiveConfig, FileSink, FileSinkConfig, LogDispatcher, LogEvent, LogPolicy, LogSink,
    RingBufferSink, Severity, Verbosity, LOG_SIZE_64_MIB,
};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

const DISABLED_CASE: &str = "disabled_builder_emit";
const FILTERED_CASE: &str = "filtered_builder_emit";
const SANITIZE_CASE: &str = "sanitize_secret_path_event";
const RING_CASE: &str = "bounded_ring_emit";
const FILE_BUFFERED_CASE: &str = "jsonl_buffered_emit";
const FILE_SYNCED_CASE: &str = "jsonl_synced_emit";
const ARCHIVE_CASE: &str = "jsonl_rotate_archive_emit";
const CASES: [&str; 7] = [
    DISABLED_CASE,
    FILTERED_CASE,
    SANITIZE_CASE,
    RING_CASE,
    FILE_BUFFERED_CASE,
    FILE_SYNCED_CASE,
    ARCHIVE_CASE,
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle");
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();

    for case in CASES {
        if selected.as_deref().is_none_or(|value| value == case) {
            run_case(case)?;
        }
    }
    contention::run(selected.as_deref())?;
    if let Some(value) = selected.as_deref() {
        if !CASES.contains(&value) && !contention::CASES.contains(&value) {
            return Err(format!("unknown appcore-log benchmark case: {value}").into());
        }
    }

    memory_checkpoint("retained");
    Ok(())
}

fn run_case(case: &str) -> Result<(), Box<dyn std::error::Error>> {
    match case {
        DISABLED_CASE => benchmark_disabled(),
        FILTERED_CASE => benchmark_filtered(),
        SANITIZE_CASE => benchmark_sanitize(),
        RING_CASE => benchmark_ring(),
        FILE_BUFFERED_CASE => benchmark_file(false)?,
        FILE_SYNCED_CASE => benchmark_file(true)?,
        ARCHIVE_CASE => benchmark_archive()?,
        _ => unreachable!("case validated by the bounded catalog"),
    }
    Ok(())
}

fn benchmark_disabled() {
    let dispatcher = LogDispatcher::new(LogPolicy::default(), Vec::new());
    let log = dispatcher.event(0, "application");

    measure(DISABLED_CASE, 2_000_000, || {
        log.info(black_box("disabled message"));
    });
}

fn benchmark_filtered() {
    let ring = Arc::new(RingBufferSink::new(1, 4096).expect("valid ring"));
    let dispatcher = LogDispatcher::new(LogPolicy::new(Verbosity::V1), vec![ring]);
    let log = dispatcher.event(0, "application").verbosity(9);

    measure(FILTERED_CASE, 2_000_000, || {
        log.debug(black_box("filtered message"));
    });
}

fn benchmark_sanitize() {
    let mut policy = LogPolicy::default();
    policy.paths.app_root = Some("/srv/application".to_string());
    let event = LogEvent::new(
        1,
        Severity::Info,
        Verbosity::V4,
        "storage",
        "request token=private completed",
    )
    .path("file", "/srv/application/data/document.json")
    .secret("authorization", "Bearer private");

    measure(SANITIZE_CASE, 100_000, || {
        black_box(policy.sanitize(black_box(&event)));
    });
}

fn benchmark_ring() {
    let ring = Arc::new(RingBufferSink::new(1024, 1024 * 1024).expect("valid ring"));
    let dispatcher = LogDispatcher::new(LogPolicy::default(), vec![ring]);
    let event = LogEvent::new(
        2,
        Severity::Info,
        Verbosity::V4,
        "application",
        "bounded operational event",
    );

    measure(RING_CASE, 100_000, || {
        dispatcher.emit(black_box(event.clone()));
    });
}

fn benchmark_file(sync_each_write: bool) -> Result<(), Box<dyn std::error::Error>> {
    let case = if sync_each_write {
        FILE_SYNCED_CASE
    } else {
        FILE_BUFFERED_CASE
    };
    let directory = benchmark_directory(case)?;
    let sink = FileSink::new(FileSinkConfig {
        path: directory.join("application.jsonl"),
        max_bytes: LOG_SIZE_64_MIB,
        sync_each_write,
        retention: 1,
        archive: None,
    })
    .expect("valid file sink");
    let event = LogEvent::new(
        3,
        Severity::Info,
        Verbosity::V4,
        "application",
        "bounded JSONL benchmark event",
    );

    measure(case, if sync_each_write { 500 } else { 50_000 }, || {
        sink.emit(black_box(&event)).expect("file emit succeeds");
    });

    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn benchmark_archive() -> Result<(), Box<dyn std::error::Error>> {
    let directory = benchmark_directory(ARCHIVE_CASE)?;
    let sink = FileSink::new(FileSinkConfig {
        path: directory.join("application.jsonl"),
        max_bytes: 512,
        sync_each_write: false,
        retention: 1,
        archive: Some(FileArchiveConfig {
            directory: directory.join("archive"),
            max_files: 16,
        }),
    })
    .expect("valid archive sink");
    let mut timestamp_ms = 1_756_944_000_000_u64;

    measure(ARCHIVE_CASE, 1_000, || {
        let event = LogEvent::new(
            timestamp_ms,
            Severity::Info,
            Verbosity::V4,
            "application",
            "event forcing frequent bounded rotation",
        );
        sink.emit(black_box(&event)).expect("archive emit succeeds");
        timestamp_ms = timestamp_ms.saturating_add(1);
    });

    std::fs::remove_dir_all(directory)?;
    Ok(())
}

fn benchmark_directory(case: &str) -> std::io::Result<PathBuf> {
    let directory = std::env::temp_dir().join(format!(
        "appcore-log-benchmark-{}-{case}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory)?;
    Ok(directory)
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn measure(case: &str, fallback: u64, mut operation: impl FnMut()) {
    let iterations = iterations(fallback);
    memory_checkpoint("workload");
    let started = Instant::now();
    for _ in 0..iterations {
        operation();
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-log::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
}

fn memory_checkpoint(phase: &str) {
    let Some(milliseconds) = checkpoint_milliseconds() else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::Write::flush(&mut std::io::stdout());
    std::thread::sleep(std::time::Duration::from_millis(milliseconds));
}

fn checkpoint_milliseconds() -> Option<u64> {
    std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()?
        .parse()
        .ok()
        .filter(|value| (1..=1_000).contains(value))
}
