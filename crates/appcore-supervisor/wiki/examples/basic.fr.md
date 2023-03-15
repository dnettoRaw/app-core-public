# Politique minimale de managed service

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Definissez un service valide et une politique de restart bornee sans demarrer
de thread.

```rust
use appcore_supervisor::{ManagedResource, RestartPolicy, ServiceDescriptor};
use std::error::Error;
use std::time::Duration;

fn main() -> Result<(), Box<dyn Error>> {
    let policy = RestartPolicy::bounded(3, Duration::from_secs(60))?
        .with_backoff(Duration::from_millis(200), Duration::from_millis(50))
        .with_shutdown_timeout(Duration::from_secs(5));
    let service = ServiceDescriptor::new("metrics", ManagedResource::Metrics, policy)?;

    println!("{}: {:?}", service.name(), service.restart_policy().mode);
    Ok(())
}
```

Le descripteur devient immuable apres construction. Les noms invalides, les
budgets de restart nuls et les deadlines de shutdown nulles echouent avant le
lifecycle.
