// =============================================================================
//        #######
//     ###       ###     F: conformance.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/08/26 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/08/26 00:00:00 by dnettoRaw
//      ###########      S: 2.0.0
// =============================================================================
// appcore-norm: test

use appcore_core::NodeId;
use appcore_storage::{StorageCapabilityProviderV1, StorageCapabilityV1};
use appcore_sync::{
    ReplicationLog, SyncCheckpointStore, SyncMessage, SyncOutbox, SyncOutboxReceipt,
};
use appcore_sync_sqlite::{
    SqliteSyncConfig, SqliteSyncError, SqliteSyncStore, SqliteSyncTombstone, SQLITE_SYNC_SCHEMA_V1,
    SQLITE_SYNC_SCHEMA_V2,
};
use rusqlite::Connection;
use std::fs;
use std::process::Command;
use std::sync::Arc;
use std::thread;
use tempfile::TempDir;

const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn open_store(root: &TempDir, name: &str) -> SqliteSyncStore {
    SqliteSyncStore::open(SqliteSyncConfig::new(root.path().join(name))).unwrap()
}

fn message(sequence: u64) -> SyncMessage {
    SyncMessage::new_simple(
        NodeId::new("sqlite-test-node").unwrap(),
        sequence,
        vec![format!("event-{sequence}").into_bytes()],
    )
}

#[test]
fn opens_wal_schema_with_conservative_capabilities() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root, "sync.db");
    let health = store.health().unwrap();
    assert_eq!(health.schema_version, SQLITE_SYNC_SCHEMA_V2);
    assert!(health.page_count > 0);
    assert!(health.page_count <= health.max_page_count);

    let descriptor = store.storage_capabilities_v1().unwrap();
    assert!(descriptor.supports(StorageCapabilityV1::Transactions));
    assert!(descriptor.supports(StorageCapabilityV1::Locking));
    assert!(descriptor.supports(StorageCapabilityV1::Snapshot));
    assert!(descriptor.supports(StorageCapabilityV1::OnlineBackup));
    assert!(descriptor.supports(StorageCapabilityV1::MultiProcess));
    assert!(!descriptor.supports(StorageCapabilityV1::Streaming));
    assert!(!descriptor.supports(StorageCapabilityV1::MultiHost));
}

#[test]
fn replication_log_is_idempotent_bounded_and_snapshot_portable() {
    let root = TempDir::new().unwrap();
    let config = SqliteSyncConfig::new(root.path().join("sync.db"))
        .with_max_read_records(2)
        .with_max_read_bytes(1024 * 1024);
    let store = SqliteSyncStore::open(config).unwrap();
    let mut log = store.replication_log();
    assert_eq!(log.append_with_sequence(b"one".to_vec(), 1), Ok(1));
    assert_eq!(log.append_with_sequence(b"one".to_vec(), 1), Ok(1));
    assert!(log.append_with_sequence(b"conflict".to_vec(), 1).is_err());
    assert_eq!(log.append_with_sequence(b"two".to_vec(), 2), Ok(2));
    assert_eq!(log.append_with_sequence(b"three".to_vec(), 3), Ok(3));
    assert!(log.events_since(0).is_err());
    assert_eq!(
        log.events_page(0, 2).unwrap(),
        vec![b"one".to_vec(), b"two".to_vec()]
    );

    let snapshot = log.create_snapshot().unwrap();
    let second = open_store(&root, "restored.db");
    let mut restored = second.replication_log();
    restored.restore_snapshot(&snapshot).unwrap();
    assert_eq!(restored.len(), Ok(3));
    assert_eq!(restored.event_at_sequence(3), Ok(Some(b"three".to_vec())));
}

#[test]
fn outbox_and_checkpoint_are_ordered_bounded_and_durable() {
    let root = TempDir::new().unwrap();
    let config = SqliteSyncConfig::new(root.path().join("sync.db")).with_max_outbox_entries(2);
    let store = SqliteSyncStore::open(config).unwrap();
    let outbox = store.outbox();
    assert_eq!(outbox.try_enqueue(message(1), 10), Ok(true));
    assert_eq!(outbox.try_enqueue(message(1), 10), Ok(false));
    let mut conflicting = message(1);
    conflicting.events = vec![b"different".to_vec()];
    assert!(outbox.try_enqueue(conflicting, 10).is_err());
    assert_eq!(outbox.try_enqueue(message(2), 10), Ok(true));
    assert_eq!(outbox.try_enqueue(message(3), 10), Ok(false));
    assert_eq!(outbox.front().unwrap().unwrap().sequence_start, 1);
    assert!(outbox.acknowledge_front("wrong").is_err());
    outbox
        .acknowledge_front("batch-sqlite-test-node-1")
        .unwrap();
    assert_eq!(outbox.front().unwrap().unwrap().sequence_start, 2);

    let checkpoints = store.checkpoint_store();
    checkpoints.set_checkpoint("peer-a", 8, HASH_A).unwrap();
    assert!(checkpoints.set_checkpoint("peer-a", 7, HASH_A).is_err());
    assert!(checkpoints
        .set_checkpoint(
            "peer-a",
            8,
            "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
        )
        .is_err());
    assert_eq!(
        checkpoints.get_checkpoint("peer-a"),
        Ok(Some((8, HASH_A.to_string())))
    );
    drop(store);

    let reopened = open_store(&root, "sync.db");
    assert_eq!(reopened.outbox().len(), Ok(1));
    assert_eq!(
        reopened.checkpoint_store().get_last_sequence("peer-a"),
        Ok(8)
    );
}

#[test]
fn outbox_pages_retry_state_and_partial_receipts_are_transactional() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root, "paged.db");
    let outbox = store.outbox();
    let messages = (1..=3).map(message).collect::<Vec<_>>();
    for message in &messages {
        assert_eq!(outbox.try_enqueue(message.clone(), 8), Ok(true));
    }
    let first_bytes = serde_json::to_vec(&messages[0]).unwrap().len();
    assert!(outbox.peek(3, first_bytes - 1).unwrap().is_empty());
    assert_eq!(
        outbox.peek(3, first_bytes).unwrap(),
        vec![messages[0].clone()]
    );
    assert!(outbox.mark_attempt(&messages[1].batch_id, 500).is_err());
    assert_eq!(outbox.mark_attempt(&messages[0].batch_id, 500), Ok(1));
    assert_eq!(outbox.mark_attempt(&messages[0].batch_id, 750), Ok(2));
    drop(outbox);
    drop(store);

    let reopened = open_store(&root, "paged.db");
    let outbox = reopened.outbox();
    assert!(outbox.next_ready(749, 3, 1_024 * 1_024).unwrap().is_empty());
    assert_eq!(outbox.next_ready(750, 3, 1_024 * 1_024).unwrap(), messages);
    let stats = outbox.stats().unwrap();
    assert_eq!(stats.pending_messages, 3);
    assert_eq!(stats.attempted_messages, Some(1));
    assert_eq!(stats.total_attempts, Some(2));
    assert_eq!(stats.next_ready_at_ms, Some(750));

    let wrong = SyncOutboxReceipt::new(vec![messages[1].batch_id.clone()]).unwrap();
    assert!(outbox.acknowledge_receipt(&wrong).is_err());
    assert_eq!(outbox.len(), Ok(3));
    let partial = SyncOutboxReceipt::new(
        messages[..2]
            .iter()
            .map(|message| message.batch_id.clone())
            .collect(),
    )
    .unwrap();
    assert_eq!(outbox.acknowledge_receipt(&partial), Ok(2));
    assert_eq!(outbox.front(), Ok(Some(messages[2].clone())));
}

#[test]
fn schema_v1_migrates_retry_columns_transactionally() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("schema-v1.db");
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA application_id = 1095779155;
             CREATE TABLE appcore_replication_log (
                 log_index INTEGER PRIMARY KEY AUTOINCREMENT,
                 source_sequence INTEGER NOT NULL CHECK(source_sequence >= 0),
                 payload BLOB NOT NULL, previous_hash TEXT NOT NULL,
                 record_hash TEXT NOT NULL);
             CREATE TABLE appcore_sync_outbox (
                 position INTEGER PRIMARY KEY AUTOINCREMENT,
                 batch_id TEXT NOT NULL UNIQUE, encoded BLOB NOT NULL);
             CREATE TABLE appcore_sync_checkpoint (
                 peer_id TEXT PRIMARY KEY, sequence INTEGER NOT NULL CHECK(sequence >= 0),
                 batch_hash TEXT NOT NULL) WITHOUT ROWID;
             CREATE TABLE appcore_sync_tombstone (
                 namespace TEXT NOT NULL, opaque_key TEXT NOT NULL,
                 deleted_sequence INTEGER NOT NULL CHECK(deleted_sequence > 0),
                 payload_hash TEXT NOT NULL,
                 expires_at_ms INTEGER NOT NULL CHECK(expires_at_ms > 0),
                 PRIMARY KEY(namespace, opaque_key)) WITHOUT ROWID;
             PRAGMA user_version = 1;",
        )
        .unwrap();
    let original = message(1);
    connection
        .execute(
            "INSERT INTO appcore_sync_outbox(batch_id, encoded) VALUES (?1, ?2)",
            rusqlite::params![original.batch_id, serde_json::to_vec(&original).unwrap()],
        )
        .unwrap();
    let version: u32 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, SQLITE_SYNC_SCHEMA_V1);
    drop(connection);

    let restored_path = root.path().join("schema-v1-restored.db");
    let report = SqliteSyncStore::restore_backup_to_new(&path, &restored_path).unwrap();
    assert_eq!(report.schema_version, SQLITE_SYNC_SCHEMA_V1);
    let restored = SqliteSyncStore::open(SqliteSyncConfig::new(restored_path)).unwrap();
    assert_eq!(
        restored.health().unwrap().schema_version,
        SQLITE_SYNC_SCHEMA_V2
    );
    assert_eq!(restored.outbox().front(), Ok(Some(original.clone())));

    let store = SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap();
    assert_eq!(
        store.health().unwrap().schema_version,
        SQLITE_SYNC_SCHEMA_V2
    );
    let outbox = store.outbox();
    assert_eq!(outbox.front(), Ok(Some(original.clone())));
    assert_eq!(outbox.stats().unwrap().total_attempts, Some(0));
    assert_eq!(outbox.mark_attempt(&original.batch_id, 50), Ok(1));
}

#[test]
fn corrupt_sqlite_retry_metadata_fails_closed() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("retry-corrupt.db");
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap();
    store.outbox().try_enqueue(message(1), 8).unwrap();
    drop(store);
    let connection = Connection::open(&path).unwrap();
    connection
        .execute_batch(
            "PRAGMA ignore_check_constraints = ON;
             UPDATE appcore_sync_outbox SET attempts = -1;",
        )
        .unwrap();
    drop(connection);
    assert!(matches!(
        SqliteSyncStore::open(SqliteSyncConfig::new(path)),
        Err(SqliteSyncError::IntegrityFailed)
    ));
}

#[test]
fn tombstones_are_opaque_monotonic_and_pruned_in_bounded_batches() {
    let root = TempDir::new().unwrap();
    let config = SqliteSyncConfig::new(root.path().join("sync.db")).with_max_tombstones(2);
    let store = SqliteSyncStore::open(config).unwrap();
    let tombstones = store.tombstone_store();
    let mut marker = SqliteSyncTombstone {
        namespace: "sync-v1".to_string(),
        opaque_key: "opaque-a".to_string(),
        deleted_sequence: 4,
        payload_hash: HASH_A.to_string(),
        expires_at_ms: 100,
    };
    assert_eq!(tombstones.record(&marker), Ok(true));
    marker.payload_hash =
        "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
    assert_eq!(
        tombstones.record(&marker),
        Err(SqliteSyncError::CorruptRecord("tombstone conflict"))
    );
    marker.payload_hash = HASH_A.to_string();
    marker.deleted_sequence = 3;
    assert_eq!(tombstones.record(&marker), Ok(false));
    marker.opaque_key = "opaque-b".to_string();
    marker.deleted_sequence = 5;
    marker.expires_at_ms = 200;
    assert_eq!(tombstones.record(&marker), Ok(true));
    marker.opaque_key = "opaque-c".to_string();
    assert_eq!(
        tombstones.record(&marker),
        Err(SqliteSyncError::CapacityExceeded("tombstone"))
    );
    assert_eq!(tombstones.active(150, 2).unwrap().len(), 1);
    assert_eq!(tombstones.prune_expired(150, 1), Ok(1));
    assert_eq!(tombstones.len(), Ok(1));
}

#[test]
fn online_backup_and_restore_publish_only_verified_new_files() {
    let root = TempDir::new().unwrap();
    let store = open_store(&root, "source.db");
    let mut log = store.replication_log();
    for sequence in 1..=10 {
        log.append_with_sequence(format!("event-{sequence}").into_bytes(), sequence)
            .unwrap();
    }
    let backup_path = root.path().join("backup.db");
    let report = store.online_backup(&backup_path).unwrap();
    assert_eq!(report.schema_version, SQLITE_SYNC_SCHEMA_V2);
    assert!(report.bytes > 0);
    let restored_path = root.path().join("restored.db");
    SqliteSyncStore::restore_backup_to_new(&backup_path, &restored_path).unwrap();
    let restored = SqliteSyncStore::open(SqliteSyncConfig::new(restored_path)).unwrap();
    assert_eq!(restored.replication_log().len(), Ok(10));
    assert_eq!(
        store.online_backup(&backup_path),
        Err(SqliteSyncError::UnsafePath)
    );
}

#[test]
fn online_backup_allows_wal_writer_progress() {
    let root = TempDir::new().unwrap();
    let store = Arc::new(open_store(&root, "source.db"));
    let mut seed = store.replication_log();
    for sequence in 1..=10 {
        seed.append_with_sequence(vec![sequence as u8], sequence)
            .unwrap();
    }
    let writer_store = Arc::clone(&store);
    let writer = thread::spawn(move || {
        let mut log = writer_store.replication_log();
        for sequence in 11..=60 {
            log.append_with_sequence(vec![sequence as u8], sequence)
                .unwrap();
        }
    });
    let backup_path = root.path().join("online.db");
    store.online_backup(&backup_path).unwrap();
    writer.join().unwrap();
    let backup = SqliteSyncStore::open(SqliteSyncConfig::new(backup_path)).unwrap();
    assert!(matches!(backup.replication_log().len(), Ok(10..=60)));
    assert_eq!(store.replication_log().len(), Ok(60));
}

#[test]
fn independent_process_profiles_share_one_sqlite_contract() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/conformance-v1.json")).unwrap();
    assert_eq!(fixture["format_version"], 1);
    for profile in fixture["profiles"].as_array().unwrap() {
        let root = TempDir::new().unwrap();
        let store = open_store(&root, &format!("{}.db", profile.as_str().unwrap()));
        let mut log = store.replication_log();
        for (offset, event) in fixture["events"].as_array().unwrap().iter().enumerate() {
            log.append_with_sequence(
                event.as_str().unwrap().as_bytes().to_vec(),
                offset as u64 + 1,
            )
            .unwrap();
        }
        store
            .checkpoint_store()
            .set_checkpoint(
                fixture["peer_id"].as_str().unwrap(),
                fixture["checkpoint_sequence"].as_u64().unwrap(),
                fixture["checkpoint_hash"].as_str().unwrap(),
            )
            .unwrap();
        assert_eq!(log.len(), Ok(3));
    }
}

#[test]
fn independent_store_instances_serialize_concurrent_writers() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("sync.db");
    let first = SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap();
    let second = SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap();
    let left = thread::spawn(move || {
        let mut log = first.replication_log();
        for sequence in 1..=50 {
            log.append_with_sequence(vec![1], sequence).unwrap();
        }
    });
    let right = thread::spawn(move || {
        let mut log = second.replication_log();
        for sequence in 51..=100 {
            log.append_with_sequence(vec![2], sequence).unwrap();
        }
    });
    left.join().unwrap();
    right.join().unwrap();
    let reopened = SqliteSyncStore::open(SqliteSyncConfig::new(path)).unwrap();
    assert_eq!(reopened.replication_log().len(), Ok(100));
}

#[test]
fn independent_processes_share_wal_without_lost_records() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("process.db");
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap();
    drop(store);
    let executable = std::env::current_exe().unwrap();
    let mut children = Vec::new();
    for (start, end) in [(1u64, 50u64), (51, 100)] {
        children.push(
            Command::new(&executable)
                .args(["--exact", "sqlite_multi_process_worker", "--nocapture"])
                .env("APPCORE_SQLITE_TEST_PATH", &path)
                .env("APPCORE_SQLITE_TEST_START", start.to_string())
                .env("APPCORE_SQLITE_TEST_END", end.to_string())
                .spawn()
                .unwrap(),
        );
    }
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    let reopened = SqliteSyncStore::open(SqliteSyncConfig::new(path)).unwrap();
    assert_eq!(reopened.replication_log().len(), Ok(100));
}

#[test]
fn sqlite_multi_process_worker() {
    let Ok(path) = std::env::var("APPCORE_SQLITE_TEST_PATH") else {
        return;
    };
    let start = std::env::var("APPCORE_SQLITE_TEST_START")
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let end = std::env::var("APPCORE_SQLITE_TEST_END")
        .unwrap()
        .parse::<u64>()
        .unwrap();
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(path)).unwrap();
    let mut log = store.replication_log();
    for sequence in start..=end {
        log.append_with_sequence(vec![sequence as u8], sequence)
            .unwrap();
    }
}

#[test]
fn unfinished_sqlite_transaction_is_rolled_back_on_reopen() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("sync.db");
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap();
    let mut log = store.replication_log();
    log.append_with_sequence(b"stable".to_vec(), 1).unwrap();
    drop(log);
    drop(store);

    let mut raw = Connection::open(&path).unwrap();
    let transaction = raw.transaction().unwrap();
    transaction
        .execute(
            "INSERT INTO appcore_replication_log
             (source_sequence, payload, previous_hash, record_hash)
             VALUES (2, X'01', '', '')",
            [],
        )
        .unwrap();
    drop(transaction);
    drop(raw);

    let reopened = SqliteSyncStore::open(SqliteSyncConfig::new(path)).unwrap();
    assert_eq!(reopened.replication_log().len(), Ok(1));
}

#[test]
fn future_or_corrupt_databases_fail_closed_without_path_diagnostics() {
    let root = TempDir::new().unwrap();
    let future_path = root.path().join("future.db");
    let future = Connection::open(&future_path).unwrap();
    future.pragma_update(None, "user_version", 2).unwrap();
    drop(future);
    assert_eq!(
        SqliteSyncStore::open(SqliteSyncConfig::new(&future_path)).unwrap_err(),
        SqliteSyncError::UpdateRequired
    );

    let corrupt_path = root.path().join("corrupt.db");
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(&corrupt_path)).unwrap();
    drop(store);
    let file = fs::OpenOptions::new()
        .write(true)
        .open(&corrupt_path)
        .unwrap();
    file.set_len(64).unwrap();
    drop(file);
    let error = SqliteSyncStore::open(SqliteSyncConfig::new(&corrupt_path)).unwrap_err();
    assert!(matches!(
        error,
        SqliteSyncError::DatabaseOperation
            | SqliteSyncError::IntegrityFailed
            | SqliteSyncError::UpdateRequired
    ));
    assert!(!error
        .to_string()
        .contains(root.path().to_string_lossy().as_ref()));
}

#[test]
fn structurally_valid_hash_chain_tampering_fails_integrity() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("tampered.db");
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap();
    let mut log = store.replication_log();
    log.append_with_sequence(b"original".to_vec(), 1).unwrap();
    drop(log);
    drop(store);

    let raw = Connection::open(&path).unwrap();
    raw.execute(
        "UPDATE appcore_replication_log SET payload = X'74616d7065726564' WHERE log_index = 1",
        [],
    )
    .unwrap();
    drop(raw);
    assert_eq!(
        SqliteSyncStore::open(SqliteSyncConfig::new(path)).unwrap_err(),
        SqliteSyncError::IntegrityFailed
    );
}

#[cfg(unix)]
#[test]
fn symlink_database_path_is_rejected() {
    use std::os::unix::fs::symlink;

    let root = TempDir::new().unwrap();
    let target = root.path().join("target.db");
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(&target)).unwrap();
    drop(store);
    let link = root.path().join("link.db");
    symlink(&target, &link).unwrap();
    assert_eq!(
        SqliteSyncStore::open(SqliteSyncConfig::new(link)).unwrap_err(),
        SqliteSyncError::UnsafePath
    );
}
