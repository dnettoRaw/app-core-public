// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures SQLite sync descriptor construction and bounded snapshot restore.

use appcore_core::NodeId;
use appcore_sync::{InMemoryReplicationLog, ReplicationLog, SyncMessage, SyncOutbox};
use std::hint::black_box;
use std::time::Instant;
use tempfile::TempDir;

const CAPABILITY_DESCRIPTOR_CASE: &str = "capability_descriptor";
const RESTORE_SNAPSHOT_CASE: &str = "restore_snapshot_32mib";
const OUTBOX_ENQUEUE_CASE: &str = "outbox_enqueue_16mib_raw";
const RESTORE_RECORDS: u64 = 32;
const RESTORE_RECORD_BYTES: usize = 1024 * 1024;
const OUTBOX_RAW_BYTES: usize = 16 * 1024 * 1024;

fn main() {
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|case| case == CAPABILITY_DESCRIPTOR_CASE)
    {
        benchmark_capability_descriptor();
    }
    if selected
        .as_deref()
        .is_none_or(|case| case == RESTORE_SNAPSHOT_CASE)
    {
        benchmark_restore_snapshot();
    }
    if selected
        .as_deref()
        .is_none_or(|case| case == OUTBOX_ENQUEUE_CASE)
    {
        benchmark_outbox_enqueue();
    }
}

fn benchmark_outbox_enqueue() {
    let root = TempDir::new().expect("benchmark temporary directory");
    let store = appcore_sync_sqlite::SqliteSyncStore::open(
        appcore_sync_sqlite::SqliteSyncConfig::new(root.path().join("outbox.db")),
    )
    .expect("benchmark store");
    let outbox = store.outbox();
    let message = SyncMessage::new(
        "batch-bench".to_string(),
        NodeId::new("node-bench").expect("benchmark node ID"),
        1,
        1,
        1,
        None,
        vec![vec![0; OUTBOX_RAW_BYTES]],
    );
    memory_checkpoint("idle");
    let iterations = iterations(1);
    let started = benchmark_started();
    for _ in 0..iterations {
        black_box(
            outbox
                .try_enqueue(black_box(message.clone()), 10_000)
                .expect("benchmark enqueue"),
        );
    }
    report(
        OUTBOX_ENQUEUE_CASE,
        iterations,
        started.elapsed().as_nanos(),
    );
}

fn benchmark_capability_descriptor() {
    memory_checkpoint("idle");
    let iterations = iterations(100_000);
    let started = benchmark_started();
    for _ in 0..iterations {
        let _ = black_box(appcore_sync_sqlite::sqlite_sync_capability_descriptor_v1());
    }
    report(
        CAPABILITY_DESCRIPTOR_CASE,
        iterations,
        started.elapsed().as_nanos(),
    );
}

fn benchmark_restore_snapshot() {
    let root = TempDir::new().expect("benchmark temporary directory");
    let mut source = InMemoryReplicationLog::new();
    for sequence in 1..=RESTORE_RECORDS {
        let mut payload = vec![0x5a; RESTORE_RECORD_BYTES];
        payload[..8].copy_from_slice(&sequence.to_be_bytes());
        source
            .append_with_sequence(payload, sequence)
            .expect("benchmark source record");
    }
    let snapshot = source.create_snapshot().expect("benchmark snapshot");
    drop(source);
    let store = appcore_sync_sqlite::SqliteSyncStore::open(
        appcore_sync_sqlite::SqliteSyncConfig::new(root.path().join("restore.db")),
    )
    .expect("benchmark store");
    let mut log = store.replication_log();
    memory_checkpoint("idle");
    let iterations = iterations(1);
    let started = benchmark_started();
    for _ in 0..iterations {
        log.restore_snapshot(black_box(&snapshot))
            .expect("benchmark snapshot restore");
    }
    black_box(log.len().expect("benchmark restored length"));
    report(
        RESTORE_SNAPSHOT_CASE,
        iterations,
        started.elapsed().as_nanos(),
    );
}

fn iterations(fallback: u64) -> u64 {
    std::env::var("APPCORE_BENCH_ITERATIONS")
        .ok()
        .and_then(|value| value.parse().ok())
        .filter(|value| *value > 0)
        .unwrap_or(fallback)
}

fn report(case: &str, iterations: u64, total_ns: u128) {
    println!(
        "appcore-sync-sqlite::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
    memory_checkpoint("retained");
}

fn benchmark_started() -> Instant {
    memory_checkpoint("workload");
    Instant::now()
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
