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
