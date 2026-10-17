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

## Fixture privada aleatória

Suporte de teste opt-in não publicado. Manter guards owner-only; sem segredos previsíveis, reparação ACL por shell ou sobrescrita. Qualificação Windows nativa fica separada.

```rust
// Enable appcore-security's test-fixtures feature in this test consumer.
use appcore_security::{test_fixtures::{random_fixture_secret, write_secret_fixture},
    PrivateDirectoryGuard, SecurityResult};
fn credential_fixture(directory: &PrivateDirectoryGuard) -> SecurityResult<()> {
    let secret = random_fixture_secret()?;
    write_secret_fixture(directory, "credential.bin", &secret)
}
```
