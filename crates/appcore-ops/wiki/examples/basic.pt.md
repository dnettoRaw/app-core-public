# Health check minimo

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Exponha um relatorio de health estavel e nao sensivel pelo contrato independente
de provider.

```rust
use appcore_ops::{BasicHealthCheck, HealthCheck, HealthReport, HealthStatus};

fn main() {
    let storage = BasicHealthCheck::new(
        "storage",
        HealthReport {
            status: HealthStatus::Healthy,
            message: Some("primary volume available".to_string()),
        },
    );
    let report = storage.check();

    println!("check={} status={:?}", storage.name(), report.status);
}
```

Use `Degraded` ou `Restricted` quando o componente continuar disponivel com
garantias reduzidas; reserve `Stopped` para indisponibilidade.
