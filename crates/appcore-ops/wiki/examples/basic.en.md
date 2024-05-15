# Minimal health check

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

Expose one stable, non-sensitive health report through the provider-neutral
health contract.

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

Use `Degraded` or `Restricted` when the component remains available with
reduced guarantees; reserve `Stopped` for an unavailable component.
