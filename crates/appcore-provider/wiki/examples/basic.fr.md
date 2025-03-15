# Provider de coordination minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Utilisez le provider deterministe en memoire et verifiez que son schema et son
health respectent le contrat de coordination courant.

```rust
use appcore_provider::{
    CoordinationStoreProvider, InMemoryCoordinationStore, ProviderResult,
};

fn main() -> ProviderResult<()> {
    let store = InMemoryCoordinationStore::default();
    store.ensure_compatible()?;

    println!("coordination schema={}", store.schema_version()?);
    Ok(())
}
```

Ce provider convient aux control planes embarques et aux tests. Les
deploiements durables doivent choisir explicitement un provider et conserver
son contrat de schema.
