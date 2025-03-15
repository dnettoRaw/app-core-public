# Minimal coordination provider

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Use the deterministic in-memory provider and verify that its schema and health
meet the current coordination contract.

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

This provider is suitable for embedded and test control planes. Durable
deployments must select a provider explicitly and preserve its schema contract.
