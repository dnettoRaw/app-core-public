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
