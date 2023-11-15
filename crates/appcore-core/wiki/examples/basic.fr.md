# Registry minimal de commandes

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Declarez les noms de commande appartenant a l'application avant de connecter
les handlers.

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

L'ordre d'enregistrement reste stable et les doublons renvoient une erreur
typee.
