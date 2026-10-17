# Batch minimo de replicacao

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Monte um batch leader-to-follower. O construtor deriva contagem, sequence final
e hash de integridade a partir dos eventos fornecidos.

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

Os eventos continuam bytes opacos da aplicacao. Receivers devem validar
sequence, contagem, hash e limites do batch antes de aplica-los.

Exemplo adicional de contrato limitado; execute num contexto que retorna Result com dependências explicitamente selecionadas.

```rust
use appcore_sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let first = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let second = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "b".into(), base_revision_id: Some("a".into()), event_id: "event-b".into() };
let chain = RevisionChainV1::new(vec![first.clone(), second.clone()])?;
assert_eq!(decide_object_revision_v1(&second, &first, &chain)?, RevisionDecisionV1::AncestorEcho);
```

## Provisionar réplica canónica

Exemplo de fonte não publicada. Reiniciar com `open_existing`; nunca provisionar para contornar estado de recuperação ausente.

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

## Consultar confirmação do master

Fonte não publicada. Evidência ausente devolve pendente; ACK de transporte não é este recibo.

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
