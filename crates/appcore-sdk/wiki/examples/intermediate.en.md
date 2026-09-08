# Intermediate SDK example

Implement `Application` when business behavior needs Runtime registration. Keep
the application contracts in the SDK and make hosting an explicit dependency.

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

The deployment process validates manifests, resolves deployment bindings, and owns shutdown.
