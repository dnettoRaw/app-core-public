# Verificar integridade e backup de snapshot

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Escreva um conjunto pequeno de dados, gere um manifest de integridade e depois
crie e verifique um backup de snapshot recuperavel.

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

`StorageManifest` detecta corrupcao acidental; nao e uma assinatura contra quem
consegue trocar arquivos e manifest. Teste o restore separadamente antes de
depender do backup.

## Exija a garantia antes do startup

O deployment pode exigir a garantia exata de snapshot sem selecionar fallback
ou alterar o artefato da aplicação:

```toml
[storage]
provider_id = "file"
settings = { required_capabilities = "snapshot" }
secret_refs = {}
```

Trocar `snapshot` por `transactions` faz o preflight manifest-first falhar
antes de abrir o provider de arquivo. Mantenha requisitos portáveis e paths,
tuning e secrets específicos no Deployment Manifest.
