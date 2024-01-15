# Minimal environment secret resolution

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Resolve secret material by opaque reference and keep it in a zeroizing,
redacted owner. Set `APPCORE_EXAMPLE_SECRET` in the deployment environment.

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

Manifests store only the reference. Restrict `as_bytes()` to the adapter that
must consume the material, and never log the returned bytes.
