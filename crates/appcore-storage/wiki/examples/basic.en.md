# Minimal atomic file storage

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Create separate data and backup roots, atomically write one relative path and
read it back.

```rust
use appcore_storage::{FileStorageProvider, StorageError, StorageResult};

fn main() -> StorageResult<()> {
    let root = std::env::temp_dir().join(format!(
        "appcore-storage-example-{}",
        std::process::id()
    ));
    let storage = FileStorageProvider::new(root.join("data"), root.join("backups"));
    storage.create_dirs()?;
    storage.write_bytes_atomic("documents/42.json", br#"{"title":"Runtime notes"}"#)?;

    let bytes = storage.read_bytes("documents/42.json")?;
    println!("stored bytes={}", bytes.len());
    std::fs::remove_dir_all(root).map_err(|_| StorageError::NotAvailable)?;
    Ok(())
}
```

Absolute paths, parent traversal and symlinks below the configured root are
rejected.
