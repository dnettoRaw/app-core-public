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
