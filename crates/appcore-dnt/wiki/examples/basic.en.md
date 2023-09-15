# Minimal encrypted DNT round trip

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Seal and open one tenant-bound payload. `APPCORE_DNT_KEY` must contain exactly
32 bytes supplied by the deployment secret boundary.

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

Opening fails closed when the application, tenant, content type, key or
authenticated bytes do not match.
