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
