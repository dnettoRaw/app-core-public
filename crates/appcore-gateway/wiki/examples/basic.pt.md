# Configuracao minima de gateway

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Faca parse das mesmas settings autenticadas usadas pelo Deployment Manifest.

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

O deployment e dono do dominio e listener. O parser rejeita settings
desconhecidas, endpoints e referencias de segredo; a autenticacao nao pode ser
desabilitada por essa fronteira.
