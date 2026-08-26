# Store SQLite sync basique

```rust
use appcore_sync_sqlite::{SqliteSyncConfig, SqliteSyncStore};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let store = SqliteSyncStore::open(SqliteSyncConfig::new("runtime/sync.db"))?;
    let health = store.health()?;
    println!("schema={} pages={}", health.schema_version, health.page_count);
    Ok(())
}
```
