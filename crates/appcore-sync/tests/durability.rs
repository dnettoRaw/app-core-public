// =============================================================================
//        #######
//     ###       ###     F: durability.rs
//    ##   ## ##   ##    P: AppCore-Runtime
//         ## ##
//                       C: 2026/10/15 10:22:10 by dnettoRaw
//    ##   ## ##   ##    U: 2026/10/15 10:24:40 by dnettoRaw
//      ###########      S: 2.0.0-alpha.6
// =============================================================================

// appcore-norm: test
//! Copy-durability tests use trusted local fixtures, not transport/authentication QA.
use appcore_core::NodeId;
use appcore_distributed_contracts::ServiceLeaderLease;
use appcore_sync::{
    sync::canonical::{
        decode_durability_receipt_v1, evaluate_durability_v1, DurabilityPolicyV1,
        DurabilityReceiptV1, DurabilityTargetV1, FileCanonicalHistoryV1, MAX_DURABILITY_RECEIPTS,
        MAX_DURABILITY_RECEIPT_BYTES,
    },
    SyncError, SyncMessage,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static SEQUENCE: AtomicU64 = AtomicU64::new(0);
struct Fixture {
    root: PathBuf,
    master: FileCanonicalHistoryV1,
    replica: FileCanonicalHistoryV1,
    target: DurabilityTargetV1,
}
impl Fixture {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "durability-{}-{unique}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(
            root.join("master"),
            include_bytes!("fixtures/canonical-history-v1.json"),
        )
        .unwrap();
        let value: serde_json::Value =
            serde_json::from_slice(include_bytes!("fixtures/canonical-history-v1.json")).unwrap();
        let scope = serde_json::from_value(value["stream"].clone()).unwrap();
        let master = FileCanonicalHistoryV1::open_existing(&root, "master", scope).unwrap();
        let target = master.durability_target(&master.head().unwrap()).unwrap();
        let replica =
            FileCanonicalHistoryV1::create(&root, "replica", target.head.stream.clone()).unwrap();
        Self {
            root,
            master,
            replica,
            target,
        }
    }
    fn master_receipt(&self) -> DurabilityReceiptV1 {
        self.master
            .durability_receipt(&self.target, "core-a")
            .unwrap()
    }
    fn copy(&self) -> DurabilityReceiptV1 {
        self.replica
            .replicate_authenticated(self.master.batches(0, 1).unwrap().pop().unwrap())
            .unwrap();
        self.replica
            .durability_receipt(&self.target, "core-b")
            .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn policy() -> DurabilityPolicyV1 {
    DurabilityPolicyV1::MasterAndReplica {
        replica_core_id: "core-b".into(),
    }
}
fn lease() -> ServiceLeaderLease {
    serde_json::from_value(
        serde_json::json!({"tenant_id":"tenant-a","cluster_id":"cluster-a",
        "service_id":"sync.writer","holder_core_id":"core-a","epoch":1,
        "acquired_at_ms":100,"expires_at_ms":10_000}),
    )
    .unwrap()
}

#[test]
fn policies_require_exact_selected_copies_without_downgrade() {
    let f = Fixture::new();
    let master = f.master_receipt();
    assert!(
        evaluate_durability_v1(
            &f.target,
            &DurabilityPolicyV1::Master,
            std::slice::from_ref(&master)
        )
        .unwrap()
        .confirmed
    );
    let waiting =
        evaluate_durability_v1(&f.target, &policy(), std::slice::from_ref(&master)).unwrap();
    assert!(!waiting.confirmed);
    assert_eq!(waiting.missing_core_ids, vec!["core-b"]);
    assert_eq!(waiting.persisted_core_ids, vec!["core-a"]);
    assert_eq!(waiting.policy, policy());
    // Empty replica / transport success cannot produce a copy receipt.
    assert!(f.replica.durability_receipt(&f.target, "core-b").is_err());
    let local = DurabilityPolicyV1::Local {
        core_id: "core-b".into(),
    };
    assert!(
        !evaluate_durability_v1(&f.target, &local, std::slice::from_ref(&master))
            .unwrap()
            .confirmed
    );
    let replica = f.copy();
    assert!(
        evaluate_durability_v1(&f.target, &local, std::slice::from_ref(&replica))
            .unwrap()
            .confirmed
    );
    assert!(
        !evaluate_durability_v1(&f.target, &policy(), std::slice::from_ref(&replica))
            .unwrap()
            .confirmed
    );
    assert!(
        evaluate_durability_v1(&f.target, &policy(), &[master, replica])
            .unwrap()
            .confirmed
    );
}

#[test]
fn lost_ack_retry_after_restart_and_later_batches_reconstructs_identical_receipt() {
    let f = Fixture::new();
    let master = f.master_receipt();
    let lost = f.copy();
    let next = SyncMessage::new(
        "next".into(),
        NodeId::new("node-a").unwrap(),
        2,
        2,
        200,
        f.target.head.batch_hash.clone(),
        vec![b"next".to_vec()],
    );
    f.master.append_authenticated(&lease(), next).unwrap();
    f.replica
        .replicate_authenticated(f.master.batches(1, 1).unwrap().pop().unwrap())
        .unwrap();
    let replica =
        FileCanonicalHistoryV1::open_existing(&f.root, "replica", f.target.head.stream.clone())
            .unwrap();
    let before = std::fs::read(f.root.join("replica")).unwrap();
    let retried = replica.durability_receipt(&f.target, "core-b").unwrap();
    assert_eq!(retried, lost);
    assert_eq!(replica.head().unwrap().sequence, 2);
    assert_eq!(std::fs::read(f.root.join("replica")).unwrap(), before);
    let confirmed =
        evaluate_durability_v1(&f.target, &policy(), &[master, retried.clone(), retried]).unwrap();
    assert!(confirmed.confirmed);
    assert_eq!(confirmed.persisted_core_ids.len(), 2);
}

#[test]
fn duplicate_or_unselected_cores_never_replace_replica_evidence() {
    let f = Fixture::new();
    let master = f.master_receipt();
    assert!(
        !evaluate_durability_v1(&f.target, &policy(), &[master.clone(), master])
            .unwrap()
            .confirmed
    );
    // The trusted host chooses its own identity, not the remote requester.
    let other = f.master.durability_receipt(&f.target, "core-c").unwrap();
    assert!(
        !evaluate_durability_v1(&f.target, &policy(), &[other])
            .unwrap()
            .confirmed
    );
    let same = DurabilityPolicyV1::MasterAndReplica {
        replica_core_id: "core-a".into(),
    };
    assert!(evaluate_durability_v1(&f.target, &same, &[]).is_err());
}

#[test]
fn mixed_heads_epochs_batch_ids_or_tenants_are_rejected() {
    let f = Fixture::new();
    let receipt = f.master_receipt();
    let mut variants = vec![receipt.clone(); 5];
    variants[0].target.head.history_hash = Some("a".repeat(64));
    variants[1].target.writer_epoch += 1;
    variants[2].target.batch_id = "other".into();
    variants[3].target.head.stream.tenant_id = "other".into();
    variants[4].target.writer_core_id = "other".into();
    for wrong in variants {
        assert!(
            evaluate_durability_v1(&f.target, &policy(), std::slice::from_ref(&wrong)).is_err()
        );
        assert!(f
            .master
            .durability_receipt(&wrong.target, "core-a")
            .is_err());
    }
}

#[test]
fn only_retained_batch_boundaries_can_be_confirmed() {
    let f = Fixture::new();
    let next = SyncMessage::new(
        "range".into(),
        NodeId::new("node-a").unwrap(),
        2,
        3,
        200,
        f.target.head.batch_hash.clone(),
        vec![vec![2], vec![3]],
    );
    let head = f.master.append_authenticated(&lease(), next).unwrap();
    let mut middle = head.clone();
    middle.sequence = 2;
    assert!(f.master.durability_target(&middle).is_err());
    let target = f.master.durability_target(&head).unwrap();
    assert_eq!(target.batch_id, "range");
    assert!(f.replica.durability_receipt(&target, "core-b").is_err());
    let mut genesis = head;
    genesis.sequence = 0;
    genesis.batch_hash = None;
    genesis.history_hash = None;
    assert!(f.master.durability_target(&genesis).is_err());
}

#[test]
fn corrupt_missing_or_locked_storage_cannot_issue_receipts() {
    let f = Fixture::new();
    f.master
        .with_verified_head(&f.target.head, |_| {
            assert!(f.master.durability_receipt(&f.target, "core-a").is_err());
            Ok(())
        })
        .unwrap();
    std::fs::write(f.root.join("master"), b"{corrupt").unwrap();
    assert!(f.master.durability_receipt(&f.target, "core-a").is_err());
    std::fs::remove_file(f.root.join("master")).unwrap();
    assert!(f.master.durability_receipt(&f.target, "core-a").is_err());
    assert!(!f.root.join("master").exists());
}

#[test]
fn version_size_count_and_identity_bounds_fail_closed() {
    let f = Fixture::new();
    let receipt = f.master_receipt();
    assert!(evaluate_durability_v1(
        &f.target,
        &policy(),
        &vec![receipt.clone(); MAX_DURABILITY_RECEIPTS + 1]
    )
    .is_err());
    assert!(decode_durability_receipt_v1(&vec![b' '; MAX_DURABILITY_RECEIPT_BYTES + 1]).is_err());
    assert!(matches!(
        decode_durability_receipt_v1(br#"{"schema_version":2,"future":true}"#),
        Err(SyncError::InvalidSyncMessage(
            "NO MORE SUPPORTED PLEASE UPDATE"
        ))
    ));
    assert!(matches!(
        decode_durability_receipt_v1(br#"{}"#),
        Err(SyncError::InvalidSyncMessage(
            "NO MORE SUPPORTED PLEASE UPDATE"
        ))
    ));
    let mut invalid = receipt;
    invalid.persisted_by_core_id = "x".repeat(129);
    assert!(invalid.validate().is_err());
    invalid.persisted_by_core_id = "core-a".into();
    invalid.schema_version = 2;
    assert!(evaluate_durability_v1(&f.target, &policy(), &[invalid]).is_err());
    let mut wrong = f.target.clone();
    wrong.head.batch_hash = None;
    assert!(evaluate_durability_v1(&wrong, &policy(), &[]).is_err());
}

#[test]
fn receipt_fixture_matches_durable_issuer_and_roundtrips_without_unknown_fields() {
    let f = Fixture::new();
    let fixture = include_bytes!("fixtures/durability-receipt-v1.json");
    let receipt = decode_durability_receipt_v1(fixture).unwrap();
    assert_eq!(receipt, f.master_receipt());
    assert_eq!(serde_json::to_vec(&receipt).unwrap(), fixture.trim_ascii());
    let mut value = serde_json::to_value(receipt).unwrap();
    value["unexpected"] = true.into();
    assert!(decode_durability_receipt_v1(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn pending_confirmation_keeps_durable_outbox_then_lost_replica_ack_is_recovered() {
    use appcore_sync::{FileSyncOutbox, SyncOutbox};
    let f = Fixture::new();
    let outbox = FileSyncOutbox::new(f.root.join("outbox")).unwrap();
    let batch = f.master.batches(0, 1).unwrap().pop().unwrap();
    outbox.try_enqueue(batch.message.clone(), 8).unwrap();
    let master = f.master_receipt();
    let pending =
        evaluate_durability_v1(&f.target, &policy(), std::slice::from_ref(&master)).unwrap();
    assert!(!pending.confirmed);
    assert_eq!(outbox.len().unwrap(), 1);
    // Replica persisted but its first receipt never reaches the caller.
    f.replica.replicate_authenticated(batch).unwrap();
    drop(outbox);
    let outbox = FileSyncOutbox::new(f.root.join("outbox")).unwrap();
    assert_eq!(outbox.front().unwrap().unwrap().batch_id, f.target.batch_id);
    let replica = f.replica.durability_receipt(&f.target, "core-b").unwrap();
    let confirmed = evaluate_durability_v1(&f.target, &policy(), &[master, replica]).unwrap();
    assert!(confirmed.confirmed);
    // This test host owns the outbox exclusively; production must serialize queue mutations
    // and durably retain policy/target/evidence before retiring the canonical batch.
    outbox
        .acknowledge_front(&confirmed.target.batch_id)
        .unwrap();
    assert_eq!(
        FileSyncOutbox::new(f.root.join("outbox"))
            .unwrap()
            .len()
            .unwrap(),
        0
    );
    assert_eq!(f.replica.batches(0, 64).unwrap().len(), 1);
}
