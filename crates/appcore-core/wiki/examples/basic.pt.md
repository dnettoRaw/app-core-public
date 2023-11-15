# Registry minimo de comandos

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Declare os nomes de comando pertencentes a aplicacao antes de ligar handlers.

```rust
use appcore_core::{CommandName, CommandRegistry, RuntimeResult};

fn main() -> RuntimeResult<()> {
    let create = CommandName::new("document.create".to_string())?;
    let archive = CommandName::new("document.archive".to_string())?;
    let mut commands = CommandRegistry::new();

    commands.register(create.clone())?;
    commands.register(archive)?;

    println!("registered={} create={}", commands.len(), commands.contains(&create));
    Ok(())
}
```

A ordem de registro e estavel e nomes duplicados retornam erro tipado.
