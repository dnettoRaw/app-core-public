# Store SQLite de sync básico

```rust
use appcore_sync_sqlite::{SqliteSyncConfig, SqliteSyncStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = SqliteSyncStore::open(SqliteSyncConfig::new("runtime/sync.db"))?;
    let health = store.health()?;
    println!("schema={} paginas={}", health.schema_version, health.page_count);
    Ok(())
}
```

Exemplo adicional de contrato limitado; execute num contexto que retorna Result com dependências explicitamente selecionadas.

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
