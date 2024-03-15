# Verifier l'integrite et le backup d'un snapshot

[English](intermediate.en.md) | [Português](intermediate.pt.md) |
[Exemple minimal](basic.fr.md) | [Guide](../guide.fr.md)

Ecrivez un petit ensemble de donnees, generez un manifest d'integrite, puis
creez et verifiez un backup de snapshot recuperable.

```rust
use appcore_storage::{
    FileStorageProvider, StorageError, StorageManifest, StorageResult,
};

fn main() -> StorageResult<()> {
    let root = std::env::temp_dir().join(format!(
        "appcore-storage-backup-example-{}",
        std::process::id()
    ));
    let data_root = root.join("data");
    let storage = FileStorageProvider::new(&data_root, root.join("backups"));
    storage.create_dirs()?;
    storage.write_bytes_atomic("documents/42.json", br#"{"revision":7}"#)?;
    storage.write_bytes_atomic("indexes/documents.idx", b"42:7\n")?;

    let manifest = StorageManifest::generate(
        "documents-app",
        "node-paris",
        "1.0.0",
        1_700_000_000_000,
        &data_root,
        &["documents/42.json", "indexes/documents.idx"],
    )?;
    manifest.verify(&data_root)?;

    let backup = storage.create_snapshot_backup("snapshot-42")?;
    storage.verify_snapshot_backup(&backup.name)?;
    println!("backup={} files={}", backup.name, manifest.files.len());

    std::fs::remove_dir_all(root).map_err(|_| StorageError::NotAvailable)?;
    Ok(())
}
```

`StorageManifest` detecte la corruption accidentelle; ce n'est pas une
signature contre un attaquant pouvant remplacer les fichiers et le manifest.
Testez le restore separement avant de dependre du backup.
