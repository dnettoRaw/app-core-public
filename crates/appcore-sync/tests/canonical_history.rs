// =============================================================================
//        #######
//     ###       ###     F: canonical_history.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 10:22:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.6
// =============================================================================

// appcore-norm: test
//! Durable logical history and co-located authority composition, not physical mesh QA.
use appcore_contracts::ServiceId;
use appcore_control_plane::FileControlPlane;
use appcore_core::{
    AppFamily, AppId, Clock, CoreIdentity, NodeId, RuntimeContractVersion, RuntimeIdentity,
    SyncGroup,
};
use appcore_distributed_contracts::{
    control_plane::lease_authority::ServiceLeaseAuthority, ControlPlaneError, ServiceLeaderLease,
};
use appcore_sync::{
    sync::canonical::{CanonicalHeadV1, CanonicalStreamV1, FileCanonicalHistoryV1},
    SyncError, SyncMessage, SyncResult,
};
use std::{
    future::Future,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    task::{Context, Poll, Waker},
};

static FIXTURE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

struct Fixture {
    root: PathBuf,
    clock: Arc<TestClock>,
    authority: FileControlPlane,
}
#[derive(Debug)]
struct TestClock(AtomicU64);
impl Clock for TestClock {
    fn now_ms(&self) -> u64 {
        self.0.load(Ordering::SeqCst)
    }
}
impl Fixture {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "canonical-{}-{unique}-{}",
            std::process::id(),
            FIXTURE_SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root).unwrap();
        let clock = Arc::new(TestClock(AtomicU64::new(100)));
        let authority =
            FileControlPlane::with_clock(root.join("authority"), 60_000, clock.clone()).unwrap();
        Self {
            root,
            clock,
            authority,
        }
    }
    fn lease(&self, node: &str) -> ServiceLeaderLease {
        ready(self.authority.acquire(
            &identity(node),
            &ServiceId::new("sync.writer").unwrap(),
            1_000,
        ))
        .unwrap()
    }
    fn store(&self, name: &str, lease: &ServiceLeaderLease) -> FileCanonicalHistoryV1 {
        FileCanonicalHistoryV1::create(
            &self.root,
            name,
            CanonicalStreamV1::from_lease(lease).unwrap(),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn ready<T>(future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(result) => result,
        Poll::Pending => panic!("reference provider unexpectedly pending"),
    }
}
fn identity(node: &str) -> CoreIdentity {
    CoreIdentity::from_runtime_defaults(RuntimeIdentity {
        app_id: AppId::new("history-test").unwrap(),
        app_family: AppFamily::new("history-test").unwrap(),
        node_id: NodeId::new(node).unwrap(),
        sync_group: SyncGroup::new("history-cluster").unwrap(),
        runtime_contract: RuntimeContractVersion::new(1),
    })
    .unwrap()
}
fn message(node: &str, seq: u64, previous: Option<String>) -> SyncMessage {
    SyncMessage::new(
        format!("{node}-{seq}"),
        NodeId::new(node).unwrap(),
        seq,
        seq,
        100,
        previous,
        vec![vec![seq as u8]],
    )
}
fn fenced_append(
    authority: &FileControlPlane,
    store: &FileCanonicalHistoryV1,
    lease: &ServiceLeaderLease,
    required: &CanonicalHeadV1,
    message: SyncMessage,
) -> SyncResult<CanonicalHeadV1> {
    store.with_verified_head(required, |transaction| {
        authority
            .with_fenced_service_write(lease, || {
                transaction
                    .append_authenticated(lease, message)
                    .map_err(|_| ControlPlaneError::Rejected("history rejected".into()))
            })
            .map_err(|_| SyncError::InvalidSyncMessage("authority refused commit"))
    })
}

#[test]
fn dead_master_takeover_keeps_logical_identity_and_exact_chain_after_both_restart() {
    let f = Fixture::new();
    let first = f.lease("node-a");
    let master = f.store("master", &first);
    let replica = f.store("replica", &first);
    let empty = master.head().unwrap();
    let m1 = message("node-a", 1, None);
    let head = fenced_append(&f.authority, &master, &first, &empty, m1.clone()).unwrap();
    let record = master.batches(0, 1).unwrap().pop().unwrap();
    assert_eq!(record.message, m1);
    assert_eq!(
        replica.replicate_authenticated(record.clone()).unwrap(),
        head
    );
    assert_eq!(replica.replicate_authenticated(record).unwrap(), head); // first ACK lost
    f.clock.0.store(1_101, Ordering::SeqCst); // master dies; grant expires
    let restarted = FileControlPlane::open_existing_with_clock(
        f.root.join("authority"),
        60_000,
        f.clock.clone(),
    )
    .unwrap();
    let second = ready(restarted.acquire(&identity("node-b"), &first.service_id, 1_000)).unwrap();
    assert!(second.epoch > first.epoch);
    let scope = CanonicalStreamV1::from_lease(&second).unwrap();
    let replica = FileCanonicalHistoryV1::open_existing(&f.root, "replica", scope.clone()).unwrap();
    let m2 = message("node-b", 2, head.batch_hash.clone());
    let next = fenced_append(&restarted, &replica, &second, &head, m2).unwrap();
    assert_eq!(next.stream, head.stream);
    assert_eq!(next.sequence, 2);
    assert!(fenced_append(
        &restarted,
        &master,
        &first,
        &head,
        message("node-a", 2, head.batch_hash.clone())
    )
    .is_err());
    // Old master rejoins as replica, preserving original new writer metadata.
    let master = FileCanonicalHistoryV1::open_existing(&f.root, "master", scope).unwrap();
    assert_eq!(
        master
            .replicate_authenticated(replica.batches(1, 1).unwrap().pop().unwrap())
            .unwrap(),
        next
    );
    assert_eq!(
        master.batches(0, 64).unwrap(),
        replica.batches(0, 64).unwrap()
    );
    let discovered = ready(restarted.inspect(&identity("node-a"), &first.service_id))
        .unwrap()
        .unwrap();
    assert_eq!(discovered, second); // address is resolved separately from stable service identity
}

#[test]
fn delayed_divergent_and_same_sequence_different_hash_candidates_cannot_run_promotion_action() {
    let f = Fixture::new();
    let lease = f.lease("node-a");
    let master = f.store("master", &lease);
    let delayed = f.store("delayed", &lease);
    let head = master
        .append_authenticated(&lease, message("node-a", 1, None))
        .unwrap();
    let mut ran = false;
    assert!(delayed
        .with_verified_head(&head, |_| {
            ran = true;
            Ok(())
        })
        .is_err());
    assert!(!ran);
    let divergent = f.store("divergent", &lease);
    divergent
        .append_authenticated(&lease, message("other", 1, None))
        .unwrap();
    assert!(divergent
        .with_verified_head(&head, |_| {
            ran = true;
            Ok(())
        })
        .is_err());
    assert!(!ran);
    let bytes = std::fs::read(f.root.join("divergent")).unwrap();
    assert!(divergent
        .replicate_authenticated(master.batches(0, 1).unwrap().pop().unwrap())
        .is_err());
    assert_eq!(std::fs::read(f.root.join("divergent")).unwrap(), bytes);
}

#[test]
fn gaps_divergent_replays_epoch_regression_and_scope_mismatch_preserve_bytes() {
    let f = Fixture::new();
    let lease = f.lease("node-a");
    let store = f.store("history", &lease);
    let m = message("node-a", 1, None);
    let head = store.append_authenticated(&lease, m.clone()).unwrap();
    let bytes = std::fs::read(f.root.join("history")).unwrap();
    assert_eq!(store.append_authenticated(&lease, m.clone()).unwrap(), head);
    for bad in [
        message("node-a", 3, head.batch_hash.clone()),
        message("node-a", 2, None),
        {
            let mut bad = m;
            bad.events[0][0] = 42;
            bad
        },
    ] {
        assert!(store.append_authenticated(&lease, bad).is_err());
    }
    let mut wrong = lease.clone();
    wrong.epoch = 0;
    assert!(store
        .append_authenticated(&wrong, message("node-a", 2, head.batch_hash.clone()))
        .is_err());
    wrong = lease.clone();
    wrong.service_id = ServiceId::new("update.coordinator").unwrap();
    assert!(store
        .append_authenticated(&wrong, message("node-a", 2, head.batch_hash.clone()))
        .is_err());
    assert_eq!(std::fs::read(f.root.join("history")).unwrap(), bytes);
}

#[test]
fn corruption_unknown_version_missing_storage_and_busy_lock_fail_closed() {
    let f = Fixture::new();
    let lease = f.lease("node-a");
    let store = f.store("history", &lease);
    store
        .append_authenticated(&lease, message("node-a", 1, None))
        .unwrap();
    let path = f.root.join("history");
    let original = std::fs::read(&path).unwrap();
    let scope = CanonicalStreamV1::from_lease(&lease).unwrap();
    let mut json: serde_json::Value = serde_json::from_slice(&original).unwrap();
    json["format_version"] = 2.into();
    std::fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
    assert!(matches!(
        store.head(),
        Err(SyncError::InvalidSyncMessage(
            "NO MORE SUPPORTED PLEASE UPDATE"
        ))
    ));
    std::fs::write(&path, &original).unwrap();
    let mut record = store.batches(0, 1).unwrap().pop().unwrap();
    record.writer = "other".into();
    let other = f.store("other", &lease);
    assert!(other.replicate_authenticated(record).is_err());
    let result = store.with_verified_head(&store.head().unwrap(), |_| {
        assert!(store.head().is_err());
        Ok(())
    });
    assert!(result.is_ok());
    std::fs::write(&path, b"{truncated").unwrap();
    assert!(store.head().is_err());
    assert!(FileCanonicalHistoryV1::create(&f.root, "history", scope.clone()).is_err());
    std::fs::remove_file(&path).unwrap();
    assert!(FileCanonicalHistoryV1::open_existing(&f.root, "history", scope).is_err());
    assert!(!path.exists());
}

#[test]
fn independent_handles_cannot_rewind_or_interleave_a_verified_head_transaction() {
    let f = Fixture::new();
    let lease = f.lease("node-a");
    let store = f.store("history", &lease);
    let scope = CanonicalStreamV1::from_lease(&lease).unwrap();
    let other = FileCanonicalHistoryV1::open_existing(&f.root, "history", scope).unwrap();
    let head = store.head().unwrap();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let signal = barrier.clone();
    std::thread::scope(|threads| {
        let required = head.clone();
        let lease = lease.clone();
        let join = threads.spawn(move || {
            store.with_verified_head(&required, |tx| {
                signal.wait();
                signal.wait();
                tx.append_authenticated(&lease, message("node-a", 1, None))
            })
        });
        barrier.wait();
        assert!(other.head().is_err());
        assert!(other
            .append_authenticated(&f.lease("node-a"), message("node-a", 1, None))
            .is_err());
        barrier.wait();
        assert_eq!(join.join().unwrap().unwrap().sequence, 1);
    });
    assert!(other.with_verified_head(&head, |_| Ok(())).is_err());
    assert_eq!(other.head().unwrap().sequence, 1);
}

#[test]
fn size_and_payload_admission_do_not_replace_existing_history() {
    let f = Fixture::new();
    let lease = f.lease("node-a");
    let store = f.store("history", &lease);
    let original = std::fs::read(f.root.join("history")).unwrap();
    let huge = SyncMessage::new(
        "huge".into(),
        NodeId::new("node-a").unwrap(),
        1,
        5,
        100,
        None,
        vec![vec![0; 1024 * 1024]; 5],
    );
    assert!(store.append_authenticated(&lease, huge).is_err());
    assert_eq!(std::fs::read(f.root.join("history")).unwrap(), original);
    assert!(store.batches(0, 65).is_err());
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(f.root.join("history"))
        .unwrap();
    file.set_len((appcore_sync::sync::canonical::MAX_CANONICAL_HISTORY_BYTES + 1) as u64)
        .unwrap();
    assert!(matches!(
        store.head(),
        Err(SyncError::InvalidSyncMessage(
            "canonical file size exceeded"
        ))
    ));
}

#[test]
fn canonical_v1_fixture_roundtrips_and_unknown_format_fails_before_schema_decode() {
    let f = Fixture::new();
    let bytes = include_bytes!("fixtures/canonical-history-v1.json");
    std::fs::write(f.root.join("fixture"), bytes).unwrap();
    let stream = CanonicalStreamV1 {
        tenant_id: "tenant-a".into(),
        cluster_id: "cluster-a".into(),
        service_id: "sync.writer".into(),
    };
    let store = FileCanonicalHistoryV1::open_existing(&f.root, "fixture", stream).unwrap();
    let head = store.head().unwrap();
    assert_eq!(head.sequence, 1);
    let record = store.batches(0, 1).unwrap().pop().unwrap();
    assert_eq!(store.replicate_authenticated(record).unwrap(), head);
    assert_eq!(std::fs::read(f.root.join("fixture")).unwrap(), bytes);
    std::fs::write(
        f.root.join("fixture"),
        br#"{"format_version":99,"future":true}"#,
    )
    .unwrap();
    assert!(matches!(
        store.head(),
        Err(SyncError::InvalidSyncMessage(
            "NO MORE SUPPORTED PLEASE UPDATE"
        ))
    ));
}

#[test]
fn unavailable_authority_blocks_writer_and_newer_epoch_blocks_old_lineage() {
    let f = Fixture::new();
    let lease = f.lease("node-a");
    let store = f.store("history", &lease);
    let empty = store.head().unwrap();
    let state = std::fs::read(f.authority.state_path()).unwrap();
    std::fs::remove_file(f.authority.state_path()).unwrap();
    assert!(fenced_append(
        &f.authority,
        &store,
        &lease,
        &empty,
        message("node-a", 1, None)
    )
    .is_err());
    assert_eq!(store.head().unwrap(), empty);
    std::fs::write(f.authority.state_path(), state).unwrap();
    let mut newer = lease.clone();
    newer.epoch = 2;
    let head = store
        .append_authenticated(&newer, message("node-a", 1, None))
        .unwrap();
    let bytes = std::fs::read(f.root.join("history")).unwrap();
    assert!(store
        .append_authenticated(&lease, message("node-a", 2, head.batch_hash.clone()))
        .is_err());
    newer.holder_core_id = identity("node-b").core_id;
    assert!(store
        .append_authenticated(&newer, message("node-b", 2, head.batch_hash))
        .is_err());
    assert_eq!(std::fs::read(f.root.join("history")).unwrap(), bytes);
}

#[test]
fn rejected_transaction_cannot_be_reused_and_capacity_never_discards_evidence() {
    let f = Fixture::new();
    let lease = f.lease("node-a");
    let store = f.store("history", &lease);
    let head = store.head().unwrap();
    store
        .with_verified_head(&head, |tx| {
            assert!(tx
                .append_authenticated(&lease, message("node-a", 2, None))
                .is_err());
            assert!(tx
                .append_authenticated(&lease, message("node-a", 1, None))
                .is_err());
            Ok(())
        })
        .unwrap();
    assert_eq!(store.head().unwrap(), head);
    store
        .append_authenticated(&lease, message("node-a", 1, None))
        .unwrap();
    let path = f.root.join("history");
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    let batch = value["batches"][0].clone();
    value["batches"] = vec![batch; appcore_sync::sync::canonical::MAX_CANONICAL_BATCHES + 1].into();
    let bytes = serde_json::to_vec(&value).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    assert!(store.head().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn callback_error_is_reconciled_from_committed_head_without_duplicate_append() {
    let f = Fixture::new();
    let lease = f.lease("node-a");
    let store = f.store("history", &lease);
    let empty = store.head().unwrap();
    let message = message("node-a", 1, None);
    let result: SyncResult<()> = store.with_verified_head(&empty, |tx| {
        tx.append_authenticated(&lease, message.clone())?;
        Err(SyncError::InvalidSyncMessage("host lost completion"))
    });
    assert!(result.is_err());
    let reopened = FileCanonicalHistoryV1::open_existing(&f.root, "history", empty.stream).unwrap();
    let committed = reopened.head().unwrap();
    assert_eq!(committed.sequence, 1);
    assert_eq!(
        reopened.append_authenticated(&lease, message).unwrap(),
        committed
    );
    assert_eq!(reopened.batches(0, 64).unwrap().len(), 1);
}
