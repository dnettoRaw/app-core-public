// =============================================================================
//        #######
//     ###       ###     F: runtime.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/31 12:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/31 12:00:00 by dnettoRaw
//      ###########      S: 1.0.1-rc.8
// =============================================================================

//! Measures bounded sync hashing and in-memory outbox admission.

use appcore_core::NodeId;
use appcore_sync::{
    FileSyncCheckpointStore, InMemoryReplicationLog, InMemorySyncCheckpointStore,
    InMemorySyncOutbox, ReplicationLog, SyncCheckpointStore, SyncMessage, SyncOutbox,
    SyncReceiverState, SYNC_CHECKPOINT_FORMAT_V1,
};
use parking_lot::Mutex;
use std::fs::{self, File};
use std::hint::black_box;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

const EVENTS_HASH_CASE: &str = "events_hash_16x128b";
const OUTBOX_ENQUEUE_CASE: &str = "in_memory_outbox_enqueue_4mib";
const RECEIVER_BATCH_IDS_CASE: &str = "receiver_processed_10000x128b";
const CHECKPOINT_LOOKUP_CASE: &str = "file_checkpoint_lookup_32768x128b";
const OUTBOX_PAYLOAD_BYTES: usize = 4 * 1_024 * 1_024;
const RECEIVER_BATCHES: usize = 10_000;
const RECEIVER_BATCH_ID_BYTES: usize = 128;
const CHECKPOINT_PEERS: usize = 32_768;
const CHECKPOINT_PEER_ID_BYTES: usize = 128;

fn main() -> Result<(), String> {
    memory_checkpoint("idle");
    let selected = std::env::var("APPCORE_BENCH_CASE").ok();
    if selected
        .as_deref()
        .is_none_or(|value| value == EVENTS_HASH_CASE)
    {
        benchmark_events_hash();
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == OUTBOX_ENQUEUE_CASE)
    {
        benchmark_outbox_enqueue()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == RECEIVER_BATCH_IDS_CASE)
    {
        benchmark_receiver_batch_ids()?;
    }
    if selected
        .as_deref()
        .is_none_or(|value| value == CHECKPOINT_LOOKUP_CASE)
    {
        benchmark_checkpoint_lookup()?;
    }
    if let Some(value) = selected.as_deref() {
        if !matches!(
            value,
            EVENTS_HASH_CASE
                | OUTBOX_ENQUEUE_CASE
                | RECEIVER_BATCH_IDS_CASE
                | CHECKPOINT_LOOKUP_CASE
        ) {
            return Err(format!("unknown appcore-sync benchmark case: {value}"));
        }
    }
    memory_checkpoint("retained");
    Ok(())
}

fn benchmark_checkpoint_lookup() -> Result<(), String> {
    let root = checkpoint_fixture_root();
    fs::create_dir(&root).map_err(|error| error.to_string())?;
    let path = root.join("checkpoints.state");
    write_checkpoint_fixture(&path)?;
    let store = FileSyncCheckpointStore::new(&path).map_err(|error| format!("{error:?}"))?;
    let target = checkpoint_peer_id(CHECKPOINT_PEERS - 1);
    let iterations = iterations(1);
    let started = benchmark_started();
    for _ in 0..iterations {
        let checkpoint = store
            .get_checkpoint(black_box(&target))
            .map_err(|error| format!("{error:?}"))?;
        if checkpoint.as_ref().map(|value| value.0) != Some(CHECKPOINT_PEERS as u64) {
            return Err("checkpoint benchmark returned the wrong sequence".to_string());
        }
        black_box(checkpoint);
    }
    report(
        CHECKPOINT_LOOKUP_CASE,
        iterations,
        started.elapsed().as_nanos(),
    );
    drop(store);
    fs::remove_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(())
}

fn checkpoint_fixture_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "appcore-sync-checkpoint-bench-{}",
        std::process::id()
    ))
}

fn write_checkpoint_fixture(path: &Path) -> Result<(), String> {
    let file = File::create(path).map_err(|error| error.to_string())?;
    let mut writer = BufWriter::new(file);
    writeln!(writer, "{SYNC_CHECKPOINT_FORMAT_V1}").map_err(|error| error.to_string())?;
    for index in 0..CHECKPOINT_PEERS {
        writeln!(
            writer,
            "{}={},{}",
            checkpoint_peer_id(index),
            index + 1,
            "a".repeat(64)
        )
        .map_err(|error| error.to_string())?;
    }
    writer.flush().map_err(|error| error.to_string())
}

fn checkpoint_peer_id(index: usize) -> String {
    let prefix = format!("peer-{index:08}-");
    format!(
        "{prefix}{}",
        "x".repeat(CHECKPOINT_PEER_ID_BYTES - prefix.len())
    )
}

fn benchmark_events_hash() {
    let source = NodeId::new("node-bench").expect("benchmark node ID must be valid");
    let events = (0_u8..16).map(|value| vec![value; 128]).collect::<Vec<_>>();
    let iterations = iterations(20_000);
    let started = benchmark_started();
    for _ in 0..iterations {
        black_box(appcore_sync::compute_events_hash(
            "batch-bench",
            &source,
            1,
            16,
            1,
            None,
            black_box(&events),
        ));
    }
    report(EVENTS_HASH_CASE, iterations, started.elapsed().as_nanos());
}

fn benchmark_outbox_enqueue() -> Result<(), String> {
    let source = NodeId::new("node-bench").map_err(|error| format!("{error:?}"))?;
    let events = (0..4)
        .map(|index| vec![u8::try_from(index).unwrap_or_default(); OUTBOX_PAYLOAD_BYTES / 4])
        .collect();
    let message = SyncMessage::new("batch-bench".to_string(), source, 1, 4, 1, None, events);
    let iterations = iterations(1);
    let started = benchmark_started();
    for _ in 0..iterations {
        let outbox = InMemorySyncOutbox::new();
        let inserted = outbox
            .try_enqueue(black_box(message.clone()), 1)
            .map_err(|error| format!("{error:?}"))?;
        if !inserted {
            return Err("in-memory outbox benchmark unexpectedly reached capacity".to_string());
        }
        black_box(outbox);
    }
    report(
        OUTBOX_ENQUEUE_CASE,
        iterations,
        started.elapsed().as_nanos(),
    );
    Ok(())
}

fn benchmark_receiver_batch_ids() -> Result<(), String> {
    let source = NodeId::new("node-bench").map_err(|error| format!("{error:?}"))?;
    let mut previous_hash = None;
    let mut messages = Vec::with_capacity(RECEIVER_BATCHES);
    for index in 1..=RECEIVER_BATCHES {
        let batch_id = format!(
            "batch-{index:08}-{}",
            "x".repeat(RECEIVER_BATCH_ID_BYTES - 15)
        );
        let message = SyncMessage::new(
            batch_id,
            source.clone(),
            index as u64,
            index as u64,
            1,
            previous_hash,
            vec![vec![u8::try_from(index % 251).unwrap_or_default()]],
        );
        previous_hash = Some(message.events_hash.clone());
        messages.push(message);
    }
    assert!(messages
        .iter()
        .all(|message| message.batch_id.len() == RECEIVER_BATCH_ID_BYTES));

    let iterations = iterations(1);
    let started = benchmark_started();
    for _ in 0..iterations {
        let log: Arc<Mutex<Box<dyn ReplicationLog + Send>>> =
            Arc::new(Mutex::new(Box::new(InMemoryReplicationLog::new())));
        let receiver = SyncReceiverState::new(log, Arc::new(InMemorySyncCheckpointStore::new()));
        for message in &messages {
            receiver
                .apply_sync_message(black_box(message))
                .map_err(|error| format!("{error:?}"))?;
        }
        black_box(receiver);
    }
    report(
        RECEIVER_BATCH_IDS_CASE,
        iterations,
        started.elapsed().as_nanos(),
    );
    Ok(())
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
        "appcore-sync::{case} iterations={iterations} total_ns={total_ns} ns_per_iter={:.2}",
        total_ns as f64 / iterations as f64
    );
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
