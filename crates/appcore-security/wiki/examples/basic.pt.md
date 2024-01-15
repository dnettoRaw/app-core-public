# Resolucao minima de secret por ambiente

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Resolva material secreto por referencia opaca e mantenha-o em um owner que
redige o `Debug` e zeroiza no drop. Defina `APPCORE_EXAMPLE_SECRET` no ambiente.

```rust
use appcore_security::{
    EnvSecretResolver, SecretResolver, SecurityResult, SecuritySecretRef,
};

fn main() -> SecurityResult<()> {
    let reference = SecuritySecretRef("APPCORE_EXAMPLE_SECRET".to_string());
    let secret = EnvSecretResolver.resolve(&reference)?;

    println!("resolved={secret:?} bytes={}", secret.as_bytes().len());
    Ok(())
}
```

Manifests guardam apenas a referencia. Restrinja `as_bytes()` ao adapter que
precisa consumir o material e nunca registre os bytes retornados.
