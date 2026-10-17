# Batch de replication minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Construisez un batch leader-to-follower. Le constructeur derive le nombre
d'evenements, la sequence finale et le hash d'integrite.

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

Les evenements restent des octets applicatifs opaques. Les receivers doivent
valider sequence, compte, hash et limites avant application.

Exemple supplémentaire de contrat borné ; exécutez dans un contexte retournant Result avec les dépendances explicitement choisies.

```rust
use appcore_sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let first = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let second = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "b".into(), base_revision_id: Some("a".into()), event_id: "event-b".into() };
let chain = RevisionChainV1::new(vec![first.clone(), second.clone()])?;
assert_eq!(decide_object_revision_v1(&second, &first, &chain)?, RevisionDecisionV1::AncestorEcho);
```

## Provisionner une réplique canonique

Exemple de source non publiée. Redémarrer avec `open_existing` ; ne jamais provisionner pour contourner un état de récupération absent.

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

## Consulter la confirmation master

Source non publiée. Une preuve absente retourne en attente ; un ACK de transport n’est pas ce reçu.

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
