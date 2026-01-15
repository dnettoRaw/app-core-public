# Configuration minimale de gateway

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Parsez les memes settings authentifiees que celles du Deployment Manifest.

```rust
use appcore_contracts::{ProviderConfig, ProviderId};
use appcore_gateway::{GatewayConfig, GATEWAY_PROVIDER_ID};
use std::error::Error;

fn main() -> Result<(), Box<dyn Error>> {
    let provider = ProviderConfig::new(ProviderId::new(GATEWAY_PROVIDER_ID)?)
        .with_setting("bind_address", "127.0.0.1:8080")?
        .with_setting("domain_suffix", "gateway.example.com")?
        .with_setting("heartbeat_interval_ms", "20000")?
        .with_setting("heartbeat_timeout_ms", "60000")?;
    let config = GatewayConfig::from_provider_config(&provider)?;

    println!(
        "bind={} auth={}",
        config.bind_address,
        config.requires_authentication()
    );
    Ok(())
}
```

Le deploiement possede le domaine et le listener. Le parser rejette les
settings inconnues, endpoints et references de secret; l'authentification ne
peut pas etre desactivee par cette frontiere.
