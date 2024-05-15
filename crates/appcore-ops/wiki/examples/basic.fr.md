# Health check minimal

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Exposez un rapport de health stable et non sensible via le contrat independant
du provider.

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

Utilisez `Degraded` ou `Restricted` si le composant reste disponible avec des
garanties reduites; reservez `Stopped` a une indisponibilite.
