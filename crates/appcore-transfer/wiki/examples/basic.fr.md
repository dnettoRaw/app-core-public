# appcore-transfer — basic

La beta `0.1.0-beta.1` transfère des objets opaques immuables depuis des sources explicitement autorisées. Les descripteurs proviennent d’une application ou d’un déploiement de confiance. L’intégrité SHA-256 n’authentifie pas une release.

```rust
use appcore_transfer::{ObjectDescriptor, TransferLimits};
use std::io::Cursor;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let bytes = b"opaque bytes";
    let limits = TransferLimits::default();
    let descriptor = ObjectDescriptor::from_reader(Cursor::new(bytes), bytes.len() as u64, 8, limits)?;
    let recovered = ObjectDescriptor::from_json(&descriptor.to_json()?, limits)?;
    assert_eq!(descriptor, recovered);
    Ok(())
}
```

No networking, hosting or activation is started.

## Politique explicite de source

API non publiée. Authentifier discovery, lier Core à endpoint, autoriser redistribution et conserver la politique objet du serveur. Checks non atomiques avec révocation.

```rust
use appcore_transfer::{AuthorizedBlockSource, BlockSource, TransferSourceAuthorizer,
    TransferSourceScopeV1, TransferResult};

fn selected_source<'a>(scope: TransferSourceScopeV1, source: &'a dyn BlockSource,
    policy: &'a dyn TransferSourceAuthorizer) -> TransferResult<AuthorizedBlockSource<'a>> {
    AuthorizedBlockSource::new(scope, source, policy)
}
```
