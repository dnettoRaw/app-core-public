# Idempotent log and portable snapshot

[Português](intermediate.pt.md) | [Français](intermediate.fr.md) |
[Minimal example](basic.en.md) | [Guide](../guide.en.md)

Append source sequences idempotently, reject conflicting reuse and restore a
validated snapshot into a fresh log.

```rust
use appcore_sync::{
    InMemoryReplicationLog, ReplicationLog, SyncError,
};

fn main() -> Result<(), String> {
    let mut primary = InMemoryReplicationLog::new();
    let first = primary
        .append_with_sequence(b"document-created".to_vec(), 42)
        .map_err(debug)?;
    let replay = primary
        .append_with_sequence(b"document-created".to_vec(), 42)
        .map_err(debug)?;
    assert_eq!(first, replay);
    assert_eq!(
        primary.append_with_sequence(b"different-event".to_vec(), 42),
        Err(SyncError::SequenceConflict(42))
    );
    primary
        .append_with_sequence(b"document-indexed".to_vec(), 43)
        .map_err(debug)?;

    let snapshot = primary.create_snapshot().map_err(debug)?;
    let mut restored = InMemoryReplicationLog::new();
    restored.restore_snapshot(&snapshot).map_err(debug)?;

    println!(
        "records={} sequence-43={}",
        restored.len(),
        restored.contains_sequence(43)
    );
    Ok(())
}

fn debug(error: impl std::fmt::Debug) -> String { format!("{error:?}") }
```

Use the file-backed log for durable replication. A snapshot is accepted only
when its version, record structure and checksum all match.

Additional bounded contract example; execute in a Result-returning context with explicitly selected dependencies.

```rust
use appcore_sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let first = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let second = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "b".into(), base_revision_id: Some("a".into()), event_id: "event-b".into() };
let chain = RevisionChainV1::new(vec![first.clone(), second.clone()])?;
assert_eq!(decide_object_revision_v1(&second, &first, &chain)?, RevisionDecisionV1::AncestorEcho);
```

## Co-located fenced continuation

The required head comes from trusted durable evidence. This example commits only history under the local authority lock, not application effects or a remote SQLite transaction. Call on a bounded blocking worker. Discover the current Core with `ServiceLeaseAuthority::inspect`; resolve and authenticate its endpoint separately.

```rust
use appcore_sync::{sync::canonical::{CanonicalHeadV1, FileCanonicalHistoryV1},
    SyncError, SyncMessage, SyncResult};
use appcore_control_plane::FileControlPlane;
use appcore_distributed_contracts::{ControlPlaneError, ServiceLeaderLease};

fn append_under_local_authority(authority: &FileControlPlane,
    history: &FileCanonicalHistoryV1, required: &CanonicalHeadV1,
    lease: &ServiceLeaderLease, message: SyncMessage) -> SyncResult<CanonicalHeadV1> {
    history.with_verified_head(required, |transaction| {
        authority.with_fenced_service_write(lease, || {
            transaction.append_authenticated(lease, message)
                .map_err(|_| ControlPlaneError::Rejected("history rejected".into()))
        }).map_err(|_| SyncError::InvalidSyncMessage("authority refused commit"))
    })
}
```

## Confirm a selected replica

The host provisions the replica Core identity. Persist the original policy/target and authenticated confirmations before reporting success or retiring an outbox item; serialize queue mutations. If replica I/O fails, keep the operation pending, record the error and retry the same target. Local canonical persistence remains committed. Re-query a historical receipt after a lost reply instead of rerunning the application handler.

```rust
use appcore_sync::{sync::canonical::{evaluate_durability_v1, CanonicalBatchV1,
    DurabilityPolicyV1, DurabilityReceiptV1, DurabilityStatusV1,
    FileCanonicalHistoryV1}, SyncResult};

// Host binds each storage identity and authenticates all transferred records/receipts.
fn copy_and_confirm(replica: &FileCanonicalHistoryV1, batch: CanonicalBatchV1,
    master_receipt: DurabilityReceiptV1, replica_core_id: &str)
    -> SyncResult<DurabilityStatusV1> {
    replica.replicate_authenticated(batch)?;
    let receipt = replica.durability_receipt(&master_receipt.target, replica_core_id)?;
    let policy = DurabilityPolicyV1::MasterAndReplica {
        replica_core_id: replica_core_id.into(),
    };
    let target = master_receipt.target.clone();
    evaluate_durability_v1(&target, &policy, &[master_receipt, receipt])
}
```
