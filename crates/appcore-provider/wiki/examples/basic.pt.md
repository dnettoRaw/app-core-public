# Provider minimo de coordenacao

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Use o provider deterministico em memoria e verifique se schema e health atendem
ao contrato atual de coordenacao.

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

Este provider serve para control planes embutidos e testes. Deployments
duraveis devem selecionar um provider explicitamente e preservar seu contrato
de schema.
