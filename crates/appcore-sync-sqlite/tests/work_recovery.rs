// =============================================================================
//        #######
//     ###       ###     F: work_recovery.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/12 00:00:00 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/12 00:00:00 by dnettoRaw
//      ###########      S: 1.0.2-rc
// =============================================================================

//! Cross-operation rollback, reopen, backup and durable conflict qualification.
use appcore_sync::*;
use appcore_sync_sqlite::*;
use tempfile::TempDir;
fn message() -> SyncMessage {
    SyncMessage::new_simple(
        appcore_core::NodeId::new("peer-a").unwrap(),
        1,
        vec![b"opaque".to_vec()],
    )
}
#[test]
fn transaction_rolls_back_all_phases_and_commits_replay_without_duplicates() {
    let root = TempDir::new().unwrap();
    let config = SqliteSyncConfig::new(root.path().join("state.db"));
    let store = SqliteSyncStore::open(config.clone()).unwrap();
    for phase in 0..3 {
        let result: SqliteSyncResult<()> = store.transaction(|work| {
            work.append(b"opaque", 1)?;
            if phase == 0 {
                return Err(SqliteSyncError::DatabaseOperation);
            }
            work.enqueue(&message())?;
            if phase == 1 {
                return Err(SqliteSyncError::DatabaseOperation);
            }
            work.checkpoint("peer-a", 1, &"11".repeat(32), 1)?;
            Err(SqliteSyncError::DatabaseOperation)
        });
        assert!(result.is_err());
        assert_eq!(store.replication_log().len(), Ok(0));
        assert_eq!(store.outbox().len(), Ok(0));
        assert_eq!(store.checkpoint_store().get_checkpoint("peer-a"), Ok(None));
    }
    for _ in 0..2 {
        store
            .transaction(|work| {
                work.append(b"opaque", 1)?;
                work.enqueue(&message())?;
                work.checkpoint("peer-a", 1, &"11".repeat(32), 1)
            })
            .unwrap();
    }
    drop(store);
    let store = SqliteSyncStore::open(config).unwrap();
    assert_eq!(store.replication_log().len(), Ok(1));
    assert_eq!(store.outbox().len(), Ok(1));
    let result = store.transaction(|work| {
        work.append(b"future", 2)?;
        work.checkpoint("peer-a", 3, &"22".repeat(32), 2)
    });
    assert!(result.is_err());
    assert_eq!(store.replication_log().len(), Ok(1));
    assert_eq!(
        store
            .checkpoint_store()
            .get_checkpoint("peer-a")
            .unwrap()
            .unwrap()
            .0,
        1
    );
}
#[test]
fn queue_capacity_failure_and_unwind_do_not_commit_log() {
    let root = TempDir::new().unwrap();
    let store = SqliteSyncStore::open(
        SqliteSyncConfig::new(root.path().join("state.db")).with_max_outbox_entries(1),
    )
    .unwrap();
    store.outbox().try_enqueue(message(), 1).unwrap();
    let other = SyncMessage::new_simple(
        appcore_core::NodeId::new("peer-a").unwrap(),
        2,
        vec![b"second".to_vec()],
    );
    assert!(store
        .transaction(|work| {
            work.append(b"second", 2)?;
            work.enqueue(&other)
        })
        .is_err());
    assert_eq!(store.replication_log().len(), Ok(0));
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _: SqliteSyncResult<()> = store.transaction(|work| {
            work.append(b"second", 2)?;
            panic!("injected crash");
        });
    }));
    assert!(result.is_err());
    assert_eq!(store.replication_log().len(), Ok(0));
}
fn conflict() -> SyncConflict {
    SyncConflict {
        conflict_id: "conflict-a".into(),
        peer_id: "peer-a".into(),
        kind: SyncConflictKind::SequencePayload,
        sequence: Some(1),
        local_hash: "11".repeat(32),
        remote_hash: "22".repeat(32),
        reason: "sequence divergence".into(),
        detected_at_ms: 100,
    }
}
#[test]
fn resolution_survives_reopen_backup_and_rejects_overwrite() {
    let root = TempDir::new().unwrap();
    let config = SqliteSyncConfig::new(root.path().join("state.db"));
    let store = SqliteSyncStore::open(config.clone()).unwrap();
    let conflicts = store.conflict_store();
    conflicts.record(conflict()).unwrap();
    let request = SyncConflictResolutionRequest {
        conflict_id: "conflict-a".into(),
        idempotency_key: "resolve-a".into(),
        resolution: SyncConflictResolution::KeepLocal,
        resolved_at_ms: 200,
    };
    assert!(!conflicts.resolve(request.clone()).unwrap().idempotent);
    let backup = root.path().join("backup.db");
    store.online_backup(&backup).unwrap();
    drop(conflicts);
    drop(store);
    for config in [config, SqliteSyncConfig::new(backup)] {
        let store = SqliteSyncStore::open(config).unwrap();
        assert!(
            store
                .conflict_store()
                .resolve(request.clone())
                .unwrap()
                .idempotent
        );
        let mut different = request.clone();
        different.resolution = SyncConflictResolution::AcceptRemote;
        assert!(matches!(
            store.conflict_store().resolve(different),
            Err(SyncError::ConflictAlreadyResolved(_))
        ));
    }
}
#[test]
fn foreign_v1_database_is_not_migrated_and_corrupt_resolution_fails_closed() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("foreign.db");
    let connection = rusqlite::Connection::open(&path).unwrap();
    connection
        .execute_batch("CREATE TABLE appcore_sync_outbox(batch_id TEXT); PRAGMA user_version=1;")
        .unwrap();
    assert_eq!(
        SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap_err(),
        SqliteSyncError::UpdateRequired
    );
    let version: i64 = connection
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .unwrap();
    assert_eq!(version, 1);
    let path = root.path().join("owned.db");
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap();
    store.conflict_store().record(conflict()).unwrap();
    drop(store);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute("UPDATE appcore_sync_conflict SET resolution='{}'", [])
        .unwrap();
    assert!(SqliteSyncStore::open(SqliteSyncConfig::new(path)).is_err());
}

#[test]
fn materialization_intent_survives_cuts_and_stays_installation_bound() {
    let root = TempDir::new().unwrap();
    let config = SqliteSyncConfig::new(root.path().join("objects.db"));
    let store = SqliteSyncStore::open(config.clone()).unwrap();
    let intent = ObjectWriteIntentV1 {
        intent_id: "write-a".into(),
        installation_id: "install-a".into(),
        object_key: "object-a".into(),
        envelope: b"opaque-envelope".to_vec(),
    };
    let result: SqliteSyncResult<()> = store.transaction(|work| {
        work.stage_object_write(&intent)?;
        work.enqueue(&message())?;
        Err(SqliteSyncError::DatabaseOperation)
    });
    assert!(result.is_err());
    assert!(store
        .pending_object_writes("install-a", 8, 1_024)
        .unwrap()
        .is_empty());
    store
        .transaction(|work| {
            work.stage_object_write(&intent)?;
            work.enqueue(&message())
        })
        .unwrap();
    drop(store);
    let store = SqliteSyncStore::open(config.clone()).unwrap();
    assert!(store
        .pending_object_writes("install-b", 8, 1_024)
        .unwrap()
        .is_empty());
    assert_eq!(
        store.pending_object_writes("install-a", 8, 1_024).unwrap(),
        vec![intent.clone()]
    );
    assert!(store
        .acknowledge_object_write("install-a", "write-a", &"00".repeat(32))
        .is_err());
    // Publish/sync the same opaque bytes twice, simulating a cut after publication
    // but before acknowledgement. No application schema or filename is inferred.
    let output = root.path().join("materialized.bin");
    for _ in 0..2 {
        let mut file = std::fs::File::create(&output).unwrap();
        std::io::Write::write_all(&mut file, &intent.envelope).unwrap();
        file.sync_all().unwrap();
        assert_eq!(std::fs::read(&output).unwrap(), intent.envelope);
    }
    assert_eq!(
        store.acknowledge_object_write("install-a", "write-a", &intent.envelope_sha256()),
        Ok(true)
    );
    assert_eq!(
        store.acknowledge_object_write("install-a", "write-a", &intent.envelope_sha256()),
        Ok(false)
    );
    drop(store);
    let store = SqliteSyncStore::open(config).unwrap();
    assert!(store
        .pending_object_writes("install-a", 8, 1_024)
        .unwrap()
        .is_empty());
    assert_eq!(store.outbox().len(), Ok(1));
}

#[test]
fn receipt_and_checkpoint_share_rollback_boundary() {
    let root = TempDir::new().unwrap();
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(root.path().join("ack.db"))).unwrap();
    let message = message();
    store.outbox().try_enqueue(message.clone(), 8).unwrap();
    let receipt = SyncOutboxReceipt::new(vec![message.batch_id.clone()]).unwrap();
    let result = store.transaction(|work| {
        work.acknowledge(&receipt)?;
        work.checkpoint("peer-a", 2, "", 1)
    });
    assert!(result.is_err());
    assert_eq!(store.outbox().len(), Ok(1));
    store
        .transaction(|work| {
            work.acknowledge(&receipt)?;
            work.checkpoint("peer-a", 1, "", 1)
        })
        .unwrap();
    assert_eq!(store.outbox().len(), Ok(0));
}

#[test]
fn killed_transaction_rolls_back_log_outbox_and_intent() {
    let root = TempDir::new().unwrap();
    let path = root.path().join("killed.db");
    let ready = root.path().join("ready");
    drop(SqliteSyncStore::open(SqliteSyncConfig::new(&path)).unwrap());
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "crash_transaction_worker", "--nocapture"])
        .env("APPCORE_WORK_CRASH_DB", &path)
        .env("APPCORE_WORK_CRASH_READY", &ready)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !ready.exists() && std::time::Instant::now() < deadline {
        assert!(
            child.try_wait().unwrap().is_none(),
            "worker exited before crash boundary"
        );
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert!(ready.exists(), "worker did not reach transaction boundary");
    child.kill().unwrap();
    child.wait().unwrap();
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(path)).unwrap();
    assert_eq!(store.replication_log().len(), Ok(0));
    assert_eq!(store.outbox().len(), Ok(0));
    assert!(store
        .pending_object_writes("install-a", 8, 1_024)
        .unwrap()
        .is_empty());
}
#[test]
fn crash_transaction_worker() {
    let Ok(path) = std::env::var("APPCORE_WORK_CRASH_DB") else {
        return;
    };
    let ready = std::env::var("APPCORE_WORK_CRASH_READY").unwrap();
    let store = SqliteSyncStore::open(SqliteSyncConfig::new(path)).unwrap();
    let _: SqliteSyncResult<()> = store.transaction(|work| {
        work.append(b"uncommitted", 1)?;
        work.enqueue(&message())?;
        work.stage_object_write(&ObjectWriteIntentV1 {
            intent_id: "write-a".into(),
            installation_id: "install-a".into(),
            object_key: "object-a".into(),
            envelope: b"uncommitted".to_vec(),
        })?;
        std::fs::write(ready, b"ready").unwrap();
        std::thread::sleep(std::time::Duration::from_secs(30));
        Err(SqliteSyncError::DatabaseOperation)
    });
}
