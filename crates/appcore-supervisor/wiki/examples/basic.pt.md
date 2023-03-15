# Politica minima de managed service

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Defina um servico validado e uma politica limitada de restart sem iniciar
threads.

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

O descriptor fica imutavel depois da construcao. Nomes invalidos, orcamento de
restart zero e deadline de shutdown zero falham antes do lifecycle.
