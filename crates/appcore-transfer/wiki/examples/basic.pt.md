# appcore-transfer — basic

A beta `0.1.0-beta.1` transfere objetos opacos imutáveis de fontes explicitamente aprovadas. Os descritores devem vir de uma aplicação ou deployment confiável. Integridade SHA-256 não autentica uma release.

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

## Política explícita da fonte

API não publicada. Autenticar discovery, vincular Core ao endpoint, autorizar redistribuição e manter política do objeto no servidor. Checks não são revogação atómica.

```rust
use appcore_transfer::{AuthorizedBlockSource, BlockSource, TransferSourceAuthorizer,
    TransferSourceScopeV1, TransferResult};

fn selected_source<'a>(scope: TransferSourceScopeV1, source: &'a dyn BlockSource,
    policy: &'a dyn TransferSourceAuthorizer) -> TransferResult<AuthorizedBlockSource<'a>> {
    AuthorizedBlockSource::new(scope, source, policy)
}
```
