# Minimal Vercel/Neon provider configuration

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Select the adapter with an HTTPS control-plane endpoint and an external bearer
token reference.

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

The Runtime receives neither a Neon connection string nor database
credentials. Neon remains behind the hosted control-plane API.
