# Storage de arquivo atomico minimo

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Crie roots separados para dados e backup, escreva um path relativo de forma
atomica e leia o conteudo.

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

Paths absolutos, parent traversal e symlinks abaixo do root configurado sao
rejeitados.
