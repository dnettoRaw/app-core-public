# Minimal replication batch

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Build one leader-to-follower batch. The constructor derives the event count,
sequence end and integrity hash from the supplied events.

```rust
use appcore_core::NodeId;
use appcore_sync::SyncMessage;

fn main() -> Result<(), String> {
    let source = NodeId::new("node-paris".to_string())
        .map_err(|error| format!("{error:?}"))?;
    let batch = SyncMessage::new(
        "batch-42".to_string(),
        source,
        42,
        43,
        1_700_000_000_000,
        None,
        vec![b"document-created".to_vec(), b"document-indexed".to_vec()],
    );

    println!(
        "batch={} events={} hash={}",
        batch.batch_id, batch.event_count, batch.events_hash
    );
    Ok(())
}
```

The events remain opaque application bytes. Receivers must validate sequence,
count, hash and configured batch bounds before applying them.

Additional bounded contract example; execute in a Result-returning context with explicitly selected dependencies.

```rust
use appcore_sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let first = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let second = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "b".into(), base_revision_id: Some("a".into()), event_id: "event-b".into() };
let chain = RevisionChainV1::new(vec![first.clone(), second.clone()])?;
assert_eq!(decide_object_revision_v1(&second, &first, &chain)?, RevisionDecisionV1::AncestorEcho);
```

## Canonical replica provisioning

Unpublished source example. Restart with `open_existing`; never provision over missing recovery state.

```rust
use appcore_sync::{sync::canonical::{CanonicalBatchV1, CanonicalStreamV1,
    FileCanonicalHistoryV1}, SyncResult};

// Initial provisioning only; authenticate stream and record before this boundary.
fn provision_replica(root: &std::path::Path, stream: CanonicalStreamV1,
    record: CanonicalBatchV1) -> SyncResult<()> {
    let replica = FileCanonicalHistoryV1::create(root, "replica-history", stream)?;
    replica.replicate_authenticated(record)?;
    Ok(())
}
```

## Inspect master confirmation

Unpublished source. Missing evidence returns pending; a transport ACK is not this receipt.

```rust
use appcore_sync::{sync::canonical::{evaluate_durability_v1, DurabilityPolicyV1,
    DurabilityReceiptV1, DurabilityStatusV1, DurabilityTargetV1}, SyncResult};

// Authenticate the complete receipt and its sender before calling.
fn master_confirmation(target: &DurabilityTargetV1,
    receipt: &DurabilityReceiptV1) -> SyncResult<DurabilityStatusV1> {
    evaluate_durability_v1(target, &DurabilityPolicyV1::Master,
        std::slice::from_ref(receipt))
}
```
