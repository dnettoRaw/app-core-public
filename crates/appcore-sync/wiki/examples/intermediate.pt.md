# Log idempotente e snapshot portavel

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Adicione sequences de origem de forma idempotente, rejeite reutilizacao com
conteudo conflitante e restaure um snapshot validado em um log novo.

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

Use o log em arquivo para replicacao duravel. Um snapshot e aceito apenas quando
versao, estrutura dos records e checksum correspondem.

Exemplo adicional de contrato limitado; execute num contexto que retorna Result com dependências explicitamente selecionadas.

```rust
use appcore_sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let first = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let second = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "b".into(), base_revision_id: Some("a".into()), event_id: "event-b".into() };
let chain = RevisionChainV1::new(vec![first.clone(), second.clone()])?;
assert_eq!(decide_object_revision_v1(&second, &first, &chain)?, RevisionDecisionV1::AncestorEcho);
```

## Continuação com fence co-localizado

O head exigido vem de evidência durável confiável. O exemplo grava apenas histórico sob lock da autoridade local, não efeitos da aplicação nem transação SQLite remota. Executar num worker bloqueante limitado. Descobrir o Core atual com `ServiceLeaseAuthority::inspect`; resolver e autenticar o endpoint separadamente.

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

## Confirmar réplica escolhida

O host provisiona a identidade Core da réplica. Persistir política/alvo originais e confirmações autenticadas antes de reportar sucesso ou retirar item da outbox; serializar mutações da fila. Se I/O da réplica falhar, manter operação pendente, registar erro e repetir o mesmo alvo. Persistência canónica local mantém-se. Consultar recibo histórico após resposta perdida, sem reexecutar handler da aplicação.

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
