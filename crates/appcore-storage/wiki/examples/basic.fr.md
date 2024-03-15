# Storage de fichier atomique minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Creez des roots distincts pour les donnees et backups, ecrivez atomiquement un
path relatif puis relisez son contenu.

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

Les paths absolus, la traversee du parent et les symlinks sous le root configure
sont refuses.
