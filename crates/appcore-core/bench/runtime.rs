// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures bounded redaction, audit snapshots, and operational journal recovery.

use appcore_core::{
    AppId, AuditCategory, AuditEntry, AuditLog, AuditOutcome, AuditRecord, CommandName, EventBus,
    EventEnvelope, EventName, FileOperationalJournal, NodeId, RuntimeLifecycle,
    RuntimeLifecycleEvent,
};
use std::hint::black_box;
use std::io::{sink, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

mod redaction_compare;

const REDACTION_CASE: &str = "redact_64b";
const JOURNAL_CASE: &str = "operational_journal_scan_256";
const AUDIT_LOG_CASE: &str = "audit_log_jsonl_snapshot_10000";
const AUDIT_JSON_CASE: &str = "audit_log_json_snapshot_10000";
const AUDIT_TAIL_OWNED_CASE: &str = "audit_log_owned_tail_1000_of_10000";
const AUDIT_TAIL_SHARED_CASE: &str = "audit_log_shared_tail_1000_of_10000";
const AUDIT_JOURNAL_SHARED_CASE: &str = "audit_log_journal_retained_3mib";
const AUDIT_JOURNAL_RESTORE_CASE: &str = "audit_log_journal_restore_3mib";
const EVENT_TAIL_OWNED_CASE: &str = "event_bus_owned_tail_1000_of_10000";
const EVENT_TAIL_SHARED_CASE: &str = "event_bus_shared_tail_1000_of_10000";
const EVENT_JOURNAL_SHARED_CASE: &str = "event_bus_journal_retained_3mib";
const LIFECYCLE_NEW_CASE: &str = "runtime_lifecycle_new";
const LIFECYCLE_STARTUP_CASE: &str = "runtime_lifecycle_startup";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    memory_checkpoint("idle", true);
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    redaction_compare::run(selected.as_deref())?;
    if selected
        .as_deref()
        .is_none_or(|value| value == REDACTION_CASE)
    {
        benchmark_redaction()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == JOURNAL_CASE)
    {
        benchmark_operational_journal()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == AUDIT_LOG_CASE)
    {
        benchmark_audit_log_export()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == AUDIT_JSON_CASE)
    {
        benchmark_audit_json_snapshot()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == AUDIT_TAIL_OWNED_CASE)
    {
        benchmark_owned_audit_tail()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == AUDIT_TAIL_SHARED_CASE)
    {
        benchmark_shared_audit_tail()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == AUDIT_JOURNAL_SHARED_CASE)
    {
        benchmark_audit_journal_retention()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == AUDIT_JOURNAL_RESTORE_CASE)
    {
        benchmark_audit_journal_restore()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == EVENT_TAIL_OWNED_CASE)
    {
        benchmark_owned_event_tail()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == EVENT_TAIL_SHARED_CASE)
    {
        benchmark_shared_event_tail()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == EVENT_JOURNAL_SHARED_CASE)
    {
        benchmark_event_journal_retention()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == LIFECYCLE_NEW_CASE)
    {
        benchmark_lifecycle_new()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == LIFECYCLE_STARTUP_CASE)
    {
        benchmark_lifecycle_startup()?;
    }
    validate_selected_case(selected.as_deref())?;
    memory_checkpoint("retained", true);
    Ok(())
}

fn validate_selected_case(selected: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    let Some(value) = selected else {
        return Ok(());
    };
    let known = [
        REDACTION_CASE,
        JOURNAL_CASE,
        AUDIT_LOG_CASE,
        AUDIT_JSON_CASE,
        AUDIT_TAIL_OWNED_CASE,
        AUDIT_TAIL_SHARED_CASE,
        AUDIT_JOURNAL_SHARED_CASE,
        AUDIT_JOURNAL_RESTORE_CASE,
        EVENT_TAIL_OWNED_CASE,
        EVENT_TAIL_SHARED_CASE,
        EVENT_JOURNAL_SHARED_CASE,
        LIFECYCLE_NEW_CASE,
        LIFECYCLE_STARTUP_CASE,
    ];
    if known.contains(&value) || redaction_compare::CASES.contains(&value) {
        return Ok(());
    }
    Err(format!("unknown appcore-core benchmark case: {value}").into())
}

fn benchmark_audit_journal_restore() -> Result<(), Box<dyn std::error::Error>> {
    let root = benchmark_root().join("audit-journal-restore");
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(&root)?;
    let journal = Arc::new(
        FileOperationalJournal::open(root.join("audit.jsonl"), 384, 16 * 1024 * 1024)
            .map_err(runtime_error)?,
    );
    let message = "x".repeat(8 * 1024);
    for index in 0..384 {
        journal
            .append_audit(
                AuditEntry::new(
                    AuditCategory::Runtime,
                    format!("restore-audit-{index}"),
                    "runtime.benchmark",
                    index,
                    index + 1,
                    AuditOutcome::Accepted,
                )
                .with_message(Some(message.clone())),
            )
            .map_err(runtime_error)?;
    }
    let result = measure(AUDIT_JOURNAL_RESTORE_CASE, 10, || {
        let log = AuditLog::new();
        log.attach_journal(Arc::clone(&journal));
        black_box(log.stats());
        Ok(())
    });
    drop(journal);
    std::fs::remove_dir_all(root)?;
    result
}

fn benchmark_audit_journal_retention() -> Result<(), Box<dyn std::error::Error>> {
    let root = benchmark_root().join("audit-journal");
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(&root)?;
    let message = "x".repeat(8 * 1024);
    let mut run = 0_u64;
    let result = measure(AUDIT_JOURNAL_SHARED_CASE, 1, || {
        let run_root = root.join(format!("run-{run}"));
        run = run.saturating_add(1);
        std::fs::create_dir_all(&run_root)?;
        let journal = Arc::new(
            FileOperationalJournal::open(run_root.join("audit.jsonl"), 384, 16 * 1024 * 1024)
                .map_err(runtime_error)?,
        );
        let log = AuditLog::new();
        log.attach_journal(Arc::clone(&journal));
        for index in 0..384 {
            log.push_entry(
                AuditEntry::new(
                    AuditCategory::Runtime,
                    format!("journal-audit-{index}"),
                    "runtime.benchmark",
                    index,
                    index + 1,
                    AuditOutcome::Accepted,
                )
                .with_message(Some(message.clone())),
            );
        }
        black_box(log.stats());
        drop(log);
        drop(journal);
        std::fs::remove_dir_all(run_root)?;
        Ok(())
    });
    std::fs::remove_dir_all(root)?;
    result
}

fn benchmark_event_journal_retention() -> Result<(), Box<dyn std::error::Error>> {
    let root = benchmark_root().join("event-journal");
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(&root)?;
    let event_name = EventName::new("runtime.benchmark".to_string()).map_err(runtime_error)?;
    let app_id = AppId::new("benchmark-app".to_string()).map_err(runtime_error)?;
    let node_id = NodeId::new("benchmark-node".to_string()).map_err(runtime_error)?;
    let mut run = 0_u64;
    let result = measure(EVENT_JOURNAL_SHARED_CASE, 1, || {
        let run_root = root.join(format!("run-{run}"));
        run = run.saturating_add(1);
        std::fs::create_dir_all(&run_root)?;
        let journal = Arc::new(
            FileOperationalJournal::open(run_root.join("events.jsonl"), 8, 16 * 1024 * 1024)
                .map_err(runtime_error)?,
        );
        let bus = EventBus::new();
        bus.attach_journal(Arc::clone(&journal));
        for index in 0..8 {
            bus.emit(
                EventEnvelope::new(
                    event_name.clone(),
                    format!("journal-event-{index}"),
                    app_id.clone(),
                    node_id.clone(),
                    index,
                    vec![index as u8; 384 * 1024],
                )
                .map_err(runtime_error)?,
            );
        }
        black_box(bus.stats());
        drop(bus);
        drop(journal);
        std::fs::remove_dir_all(run_root)?;
        Ok(())
    });
    std::fs::remove_dir_all(root)?;
    result
}

fn benchmark_lifecycle_new() -> Result<(), Box<dyn std::error::Error>> {
    measure(LIFECYCLE_NEW_CASE, 10_000, || {
        black_box(RuntimeLifecycle::new());
        Ok(())
    })
}

fn benchmark_lifecycle_startup() -> Result<(), Box<dyn std::error::Error>> {
    measure(LIFECYCLE_STARTUP_CASE, 10_000, || {
        let lifecycle = RuntimeLifecycle::new();
        lifecycle
            .apply(RuntimeLifecycleEvent::ConfigLoaded)
            .map_err(runtime_error)?;
        lifecycle
            .apply(RuntimeLifecycleEvent::SecurityChecked)
            .map_err(runtime_error)?;
        lifecycle
            .apply(RuntimeLifecycleEvent::StorageOpened)
            .map_err(runtime_error)?;
        black_box(
            lifecycle
                .apply(RuntimeLifecycleEvent::ApiStarted)
                .map_err(runtime_error)?,
        );
        Ok(())
    })
}

fn benchmark_owned_event_tail() -> Result<(), Box<dyn std::error::Error>> {
    let bus = event_query_fixture()?;
    measure(EVENT_TAIL_OWNED_CASE, 10, || {
        let events = bus.events();
        let checksum = events
            .iter()
            .rev()
            .take(1_000)
            .map(|item| item.event_id.len() + item.payload.len())
            .sum::<usize>();
        black_box(checksum);
        Ok(())
    })
}

fn benchmark_shared_event_tail() -> Result<(), Box<dyn std::error::Error>> {
    let bus = event_query_fixture()?;
    measure(EVENT_TAIL_SHARED_CASE, 10, || {
        let events = bus.snapshot();
        let checksum = events
            .recent(1_000)
            .map(|item| item.event_id.len() + item.payload.len())
            .sum::<usize>();
        black_box(checksum);
        Ok(())
    })
}

fn event_query_fixture() -> Result<EventBus, Box<dyn std::error::Error>> {
    let bus = EventBus::new();
    let event_name = EventName::new("runtime.benchmark".to_string()).map_err(runtime_error)?;
    let app_id = AppId::new("benchmark-app".to_string()).map_err(runtime_error)?;
    let node_id = NodeId::new("benchmark-node".to_string()).map_err(runtime_error)?;
    for index in 0..10_000 {
        bus.emit(
            EventEnvelope::new(
                event_name.clone(),
                format!("event-{index}"),
                app_id.clone(),
                node_id.clone(),
                index,
                vec![index as u8; 256],
            )
            .map_err(runtime_error)?,
        );
    }
    if bus.stats().event_count != 10_000 {
        return Err("default event budget did not retain the benchmark fixture".into());
    }
    Ok(bus)
}

fn benchmark_owned_audit_tail() -> Result<(), Box<dyn std::error::Error>> {
    let log = audit_query_fixture()?;
    measure(AUDIT_TAIL_OWNED_CASE, 10, || {
        let records = log.records();
        let entries = log.entries();
        let checksum = records
            .iter()
            .rev()
            .take(1_000)
            .map(|item| item.command_id.len())
            .sum::<usize>()
            + entries
                .iter()
                .rev()
                .take(1_000)
                .map(|item| item.operation_id.len())
                .sum::<usize>();
        black_box(checksum);
        Ok(())
    })
}

fn benchmark_shared_audit_tail() -> Result<(), Box<dyn std::error::Error>> {
    let log = audit_query_fixture()?;
    measure(AUDIT_TAIL_SHARED_CASE, 10, || {
        let records = log.records_snapshot();
        let entries = log.entries_snapshot();
        let checksum = records
            .recent(1_000)
            .map(|item| item.command_id.len())
            .sum::<usize>()
            + entries
                .recent(1_000)
                .map(|item| item.operation_id.len())
                .sum::<usize>();
        black_box(checksum);
        Ok(())
    })
}

fn audit_query_fixture() -> Result<AuditLog, Box<dyn std::error::Error>> {
    let log = AuditLog::new();
    let command_name = CommandName::new("runtime.benchmark".to_string()).map_err(runtime_error)?;
    let app_id = AppId::new("benchmark-app".to_string()).map_err(runtime_error)?;
    let node_id = NodeId::new("benchmark-node".to_string()).map_err(runtime_error)?;
    for index in 0..10_000 {
        log.push(AuditRecord {
            command_id: format!("command-{index}"),
            command_name: command_name.clone(),
            app_id: app_id.clone(),
            node_id: node_id.clone(),
            timestamp_ms: index,
            outcome: AuditOutcome::Accepted,
            message: Some(format!("bounded benchmark audit message {index}")),
            trace: None,
        });
    }
    let stats = log.stats();
    if stats.record_count != 10_000 || stats.entry_count != 10_000 {
        return Err("default audit budget did not retain the query fixture".into());
    }
    Ok(log)
}

fn benchmark_audit_log_export() -> Result<(), Box<dyn std::error::Error>> {
    let log = audit_log_fixture()?;
    measure(AUDIT_LOG_CASE, 10, || {
        log.write_jsonl(&mut sink())?;
        black_box(log.stats());
        Ok(())
    })
}

fn benchmark_audit_json_snapshot() -> Result<(), Box<dyn std::error::Error>> {
    let log = audit_log_fixture()?;
    let snapshot = log.entries_snapshot();
    let mut counter = ByteCounter::default();
    serde_json::to_writer_pretty(&mut counter, &snapshot)?;
    println!(
        "appcore-core::{AUDIT_JSON_CASE} fixture_bytes={}",
        counter.bytes
    );
    measure(AUDIT_JSON_CASE, 10, || {
        serde_json::to_writer_pretty(&mut sink(), &snapshot)?;
        black_box(snapshot.len());
        Ok(())
    })
}

fn audit_log_fixture() -> Result<AuditLog, Box<dyn std::error::Error>> {
    let log = AuditLog::new();
    for index in 0..10_000 {
        log.push_entry(AuditEntry::new(
            AuditCategory::Runtime,
            format!("bench-{index}"),
            "runtime.audit-export",
            index,
            index + 1,
            AuditOutcome::Accepted,
        ));
    }
    if log.stats().entry_count != 10_000 {
        return Err("default audit budget did not retain the benchmark fixture".into());
    }
    Ok(log)
}

#[derive(Default)]
struct ByteCounter {
    bytes: usize,
}

impl Write for ByteCounter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("benchmark byte count overflow"))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn benchmark_redaction() -> Result<(), Box<dyn std::error::Error>> {
    let input = "authorization=Bearer <redacted> password=<redacted> tenant=bench";
    measure(REDACTION_CASE, 100_000, || {
        black_box(appcore_core::redact_text_with_limit(input, 256));
        Ok(())
    })
}

fn benchmark_operational_journal() -> Result<(), Box<dyn std::error::Error>> {
    let root = benchmark_root();
    if root.exists() {
        std::fs::remove_dir_all(&root)?;
    }
    std::fs::create_dir_all(&root)?;
    let path = root.join("operational.jsonl");
    let journal =
        FileOperationalJournal::open(&path, 256, 8 * 1024 * 1024).map_err(runtime_error)?;
    for index in 0..256 {
        journal
            .append_audit(AuditEntry::new(
                AuditCategory::Runtime,
                format!("bench-{index}"),
                "runtime.benchmark",
                index,
                index + 1,
                AuditOutcome::Accepted,
            ))
            .map_err(runtime_error)?;
    }
    drop(journal);
    let result = measure(JOURNAL_CASE, 100, || {
        let journal =
            FileOperationalJournal::open(&path, 256, 8 * 1024 * 1024).map_err(runtime_error)?;
        black_box(journal);
        Ok(())
    });
    std::fs::remove_dir_all(root)?;
    result
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn measure(
    case_name: &str,
    fallback_iterations: u64,
    mut operation: impl FnMut() -> Result<(), Box<dyn std::error::Error>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let iterations = iterations(fallback_iterations);
    memory_checkpoint("workload", true);
    let started = Instant::now();
    for _ in 0..iterations {
        operation()?;
    }
    let total_ns = started.elapsed().as_nanos();
    println!(
        "appcore-core::{case_name} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    Ok(())
}

fn benchmark_root() -> PathBuf {
    std::env::temp_dir().join(format!("appcore-core-benchmark-{}", std::process::id()))
}

fn runtime_error(error: appcore_core::RuntimeError) -> Box<dyn std::error::Error> {
    Box::new(std::io::Error::other(format!("{error:?}")))
}

fn memory_checkpoint(phase: &str, settle: bool) {
    let Some(milliseconds) = checkpoint_milliseconds() else {
        return;
    };
    println!(
        "appcore-bench-memory phase={phase} pid={}",
        std::process::id()
    );
    let _ = std::io::stdout().flush();
    if settle {
        std::thread::sleep(std::time::Duration::from_millis(milliseconds));
    }
}
fn checkpoint_milliseconds() -> Option<u64> {
    std::env::var("APPCORE_BENCH_MEMORY_CHECKPOINT_MS")
        .ok()?
        .parse()
        .ok()
        .filter(|value| (1..=1_000).contains(value))
}
