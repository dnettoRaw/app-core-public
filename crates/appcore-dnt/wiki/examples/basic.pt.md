# Round trip DNT criptografado minimo

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Sele e abra um payload vinculado ao tenant. `APPCORE_DNT_KEY` deve conter
exatamente 32 bytes fornecidos pelo limite de secrets do deployment.

```rust
use appcore_contracts::ApplicationId;
use appcore_dnt::{
    open, seal, BytesCodec, ContentType, DntOpenOptions, DntSealOptions, KeyId,
    SecretKey, StaticDntKeyProvider,
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

    let envelope = seal(
        br#"{"revision":7}"#,
        &keys,
        &BytesCodec,
        DntSealOptions {
            application_id: application_id.clone(),
            tenant_id: Some(tenant_id.clone()),
            content_type: content_type.clone(),
            schema_version: 1,
            key_id,
            created_at_ms: 1_700_000_000_000,
            public_metadata: b"kind=snapshot".to_vec(),
            encrypted_metadata: b"source=primary".to_vec(),
            flags: 0,
            max_payload_bytes: Some(1024 * 1024),
        },
    )?;
    let opened = open(
        &envelope,
        &keys,
        &BytesCodec,
        &DntOpenOptions {
            application_id,
            tenant_id: Some(tenant_id),
            content_type,
            max_payload_bytes: Some(1024 * 1024),
        },
    )?;

    println!("authenticated payload bytes={}", opened.payload.len());
    Ok(())
}
```

A abertura falha de forma fechada quando aplicacao, tenant, content type, chave
ou bytes autenticados nao correspondem.
