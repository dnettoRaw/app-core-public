# Arquivo de snapshot criptografado e atomico

[English](intermediate.en.md) | [Français](intermediate.fr.md) |
[Exemplo minimo](basic.pt.md) | [Guia](../guide.pt.md)

Persista um snapshot limitado pela API de arquivo atomico verificado. O leitor
autentica o envelope completo antes de retornar plaintext.

```rust
use appcore_contracts::ApplicationId;
use appcore_dnt::{
    read_verified, write_atomic, BytesCodec, ContentType, DntOpenOptions,
    DntSealOptions, KeyId, SecretKey, StaticDntKeyProvider,
};
use appcore_types::TenantId;
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let key = std::env::var("APPCORE_DNT_KEY")?;
    let key_id = KeyId::new("notes-key-v1")?;
    let keys = StaticDntKeyProvider::new()
        .with_key(key_id.clone(), SecretKey::from_slice(key.as_bytes())?);
    let application_id = ApplicationId::new("notes-app")?;
    let tenant_id = TenantId::new("tenant-acme")
        .map_err(|error| format!("{error:?}"))?;
    let content_type = ContentType::new("application/vnd.notes.snapshot")?;
    let open_options = DntOpenOptions {
        application_id: application_id.clone(),
        tenant_id: Some(tenant_id.clone()),
        content_type: content_type.clone(),
        max_payload_bytes: Some(4 * 1024 * 1024),
    };
    let path = std::env::temp_dir().join(format!(
        "appcore-dnt-example-{}.dnt",
        std::process::id()
    ));

    write_atomic(
        &path,
        br#"{"revision":42,"notes":128}"#,
        &keys,
        &BytesCodec,
        DntSealOptions {
            application_id,
            tenant_id: Some(tenant_id),
            content_type,
            schema_version: 1,
            key_id,
            created_at_ms: 1_700_000_000_000,
            public_metadata: b"kind=snapshot".to_vec(),
            encrypted_metadata: b"checkpoint=42".to_vec(),
            flags: 0,
            max_payload_bytes: Some(4 * 1024 * 1024),
        }
        .compact_payload(),
        &open_options,
    )?;

    let mut opened = read_verified(&path, &keys, &BytesCodec, &open_options)?;
    println!("schema={} bytes={}", opened.header.schema_version, opened.payload.len());
    opened.zeroize_plaintext();
    std::fs::remove_file(path)?;
    Ok(())
}
```

Em producao, use um path pertencente a instalacao. `write_atomic` verifica o
novo envelope antes da troca e `read_verified` rejeita symlinks e arquivos
grandes demais.
