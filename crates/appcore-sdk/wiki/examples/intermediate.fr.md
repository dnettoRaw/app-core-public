# Exemple SDK intermédiaire

Implémentez `Application` lorsque le comportement doit enregistrer des éléments
du Runtime. Gardez les contrats applicatifs dans le SDK et rendez le host explicite.

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

Le processus de déploiement valide les manifestes, résout les bindings et gère l'arrêt.
