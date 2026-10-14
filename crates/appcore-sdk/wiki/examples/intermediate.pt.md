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

Exemplo adicional de contrato limitado; execute num contexto que retorna Result com dependências explicitamente selecionadas.

```rust
use appcore_sdk::sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let revision = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let chain = RevisionChainV1::new(vec![revision.clone()])?;
assert_eq!(decide_object_revision_v1(&revision, &revision, &chain)?, RevisionDecisionV1::AlreadyApplied);
```
