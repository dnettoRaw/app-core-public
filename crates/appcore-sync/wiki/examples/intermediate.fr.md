# Log idempotent et snapshot portable

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Ajoutez les sequences source de maniere idempotente, refusez une reutilisation
conflictuelle et restaurez un snapshot valide dans un nouveau log.

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

Utilisez le log fichier pour une replication durable. Un snapshot n'est accepte
que si sa version, sa structure et son checksum correspondent.

Exemple supplémentaire de contrat borné ; exécutez dans un contexte retournant Result avec les dépendances explicitement choisies.

```rust
use appcore_sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let first = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let second = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "b".into(), base_revision_id: Some("a".into()), event_id: "event-b".into() };
let chain = RevisionChainV1::new(vec![first.clone(), second.clone()])?;
assert_eq!(decide_object_revision_v1(&second, &first, &chain)?, RevisionDecisionV1::AncestorEcho);
```

## Continuation avec fence colocalisé

La tête exigée provient d’une preuve durable fiable. Cet exemple écrit uniquement l’historique sous le verrou de l’autorité locale, pas les effets applicatifs ni une transaction SQLite distante. Exécuter dans un worker bloquant borné. Découvrir le Core courant avec `ServiceLeaseAuthority::inspect`, puis résoudre et authentifier son endpoint séparément.

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

## Confirmer une réplique choisie

Le host provisionne l’identité Core de la réplique. Persister politique/cible originales et confirmations authentifiées avant succès ou retrait d’outbox ; sérialiser les mutations. Si les I/O réplique échouent, garder l’opération en attente, enregistrer l’erreur et répéter la même cible. La persistance canonique locale reste acquise. Consulter le reçu historique après réponse perdue sans réexécuter le handler applicatif.

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
