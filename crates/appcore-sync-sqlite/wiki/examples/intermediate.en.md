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

Additional bounded contract example; execute in a Result-returning context with explicitly selected dependencies.

```rust
use appcore_sync_sqlite::{ObjectWriteIntentV1, SqliteSyncConfig, SqliteSyncStore};
let store = SqliteSyncStore::open(SqliteSyncConfig::new("isolated-sync.db"))?;
let intent = ObjectWriteIntentV1 { intent_id: "write-a".into(), installation_id: "install-a".into(), object_key: "object-a".into(), envelope: b"opaque-envelope".to_vec() };
store.transaction(|work| {
    work.append(&intent.envelope, 1)?;
    work.stage_object_write(&intent)?;
    work.checkpoint("peer-a", 1, "", 1)
})?;
assert_eq!(store.pending_object_writes("install-a", 8, 1024)?.len(), 1);
```
