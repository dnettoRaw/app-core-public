# Intermediate bounded sync workflow

```rust
use appcore_core::NodeId;
use appcore_sync::{ReplicationLog, SyncCheckpointStore, SyncMessage, SyncOutbox};
use appcore_sync_sqlite::{SqliteSyncConfig, SqliteSyncStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = SqliteSyncConfig::new("runtime/sync.db")
        .with_max_outbox_entries(1_024)
        .with_max_connections(8);
    let store = SqliteSyncStore::open(config)?;
    let mut log = store.replication_log();
    log.append_with_sequence(b"opaque-runtime-event".to_vec(), 1)?;

    let message = SyncMessage::new_simple(
        NodeId::new("node-a")?,
        1,
        log.events_page(0, 128)?,
    );
    let outbox = store.outbox();
    if outbox.try_enqueue(message, 1_024)? {
        store.checkpoint_store().set_last_sequence("peer-a", 1)?;
    }
    store.online_backup("runtime/sync-backup.db")?;
    Ok(())
}
```
