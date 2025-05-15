# Configuration minimale du provider Vercel/Neon

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Selectionnez l'adapter avec un endpoint HTTPS de control plane et une reference
externe au token bearer.

```rust
use appcore_contracts::{ProviderConfig, ProviderId, SecretRef};
use appcore_provider_vercel_neon::{
    AUTH_TOKEN_SECRET, VERCEL_NEON_PROVIDER_ID,
};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let config = ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID)?)
        .with_endpoint("https://control.example.com")?
        .with_secret_ref(
            AUTH_TOKEN_SECRET,
            SecretRef::new("env:APPCORE_CONTROL_TOKEN")?,
        )?
        .with_setting("timeout_ms", "5000")?;

    println!(
        "provider={} endpoint={}",
        config.provider_id().as_str(),
        config.endpoint().unwrap_or("missing")
    );
    Ok(())
}
```

Le Runtime ne recoit ni connection string Neon ni credentials de base de
donnees. Neon reste derriere l'API de control plane hebergee.
