# Exemplo intermediário do SDK

Implemente `Application` quando o comportamento precisar de registros do
Runtime. Mantenha contratos da aplicação no SDK e torne o host explícito.

```rust
use appcore_sdk::application::{CommandName, CommandRegistry, NodeId, RuntimeResult};
use appcore_sdk::{App, Application, AppResult};

struct ReportingApp;

impl Application for ReportingApp {
    fn register_commands(&self, registry: &mut CommandRegistry) -> RuntimeResult<()> {
        registry.register(CommandName::new("report.generate")?)
    }
}

fn main() -> AppResult<()> {
    let app = App::new("reporting")?;
    let prepared = app.prepare(&ReportingApp, NodeId::new("reporting-local")?)?;
    assert_eq!(prepared.runtime().commands().len(), 1);
    Ok(())
}
```

O processo de deployment valida manifestos, resolve bindings e controla shutdown.
