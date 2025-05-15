# Configuracao minima do provider Vercel/Neon

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Selecione o adapter com endpoint HTTPS do control plane e referencia externa do
token bearer.

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

O Runtime nao recebe connection string do Neon nem credenciais de banco. Neon
permanece atras da API hospedada do control plane.
