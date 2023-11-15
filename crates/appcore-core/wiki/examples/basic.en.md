# Minimal command registry

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Declare the command names owned by an application before wiring handlers.

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

Registration order is stable and duplicate names return a typed error.
