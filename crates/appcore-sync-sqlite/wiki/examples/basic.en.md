# Basic SQLite sync store

```rust
use appcore_sync_sqlite::{SqliteSyncConfig, SqliteSyncStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = SqliteSyncStore::open(SqliteSyncConfig::new("runtime/sync.db"))?;
    let health = store.health()?;
    println!("schema={} pages={}", health.schema_version, health.page_count);
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
