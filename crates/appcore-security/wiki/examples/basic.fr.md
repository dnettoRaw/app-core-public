# Resolution minimale d'un secret d'environnement

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Resolvez le materiel secret par reference opaque et gardez-le dans un owner qui
redige `Debug` et zeroise au drop. Definissez `APPCORE_EXAMPLE_SECRET` dans
l'environnement.

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

Les manifests ne stockent que la reference. Limitez `as_bytes()` a l'adapter
qui consomme le materiel et ne journalisez jamais les octets retournes.

## Fixture privée aléatoire

Support de tests opt-in non publié. Conserver les guards owner-only ; ni secrets prévisibles, ni réparation ACL par shell, ni écrasement. Qualification Windows native séparée.

```rust
// Enable appcore-security's test-fixtures feature in this test consumer.
use appcore_security::{test_fixtures::{random_fixture_secret, write_secret_fixture},
    PrivateDirectoryGuard, SecurityResult};
fn credential_fixture(directory: &PrivateDirectoryGuard) -> SecurityResult<()> {
    let secret = random_fixture_secret()?;
    write_secret_fixture(directory, "credential.bin", &secret)
}
```
