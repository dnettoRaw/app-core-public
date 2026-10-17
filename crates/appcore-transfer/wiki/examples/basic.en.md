# appcore-transfer — basic

Beta `0.1.0-beta.1` transfers immutable opaque objects from explicitly approved sources. Descriptors must come from a trusted application or deployment. SHA-256 integrity does not authenticate a release.

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

## Explicit source policy

Unpublished API. Authenticate discovery, bind Core to endpoint, authorize redistribution and retain mandatory server object policy. Checks are not atomic revocation.

```rust
use appcore_transfer::{AuthorizedBlockSource, BlockSource, TransferSourceAuthorizer,
    TransferSourceScopeV1, TransferResult};

fn selected_source<'a>(scope: TransferSourceScopeV1, source: &'a dyn BlockSource,
    policy: &'a dyn TransferSourceAuthorizer) -> TransferResult<AuthorizedBlockSource<'a>> {
    AuthorizedBlockSource::new(scope, source, policy)
}
```
