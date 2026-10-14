# Exemplo básico do SDK

```rust
use appcore_sdk::prelude::*;

fn main() -> AppResult<()> {
    appcore_sdk::run("hello-world", |app| {
        app.log("Olá, mundo!");
        Ok(())
    })
}
```

Este exemplo usa uma dependência e não faz I/O além do log explícito.

Exemplo adicional de contrato limitado; execute num contexto que retorna Result com dependências explicitamente selecionadas.

```rust
use appcore_sdk::sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let revision = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let chain = RevisionChainV1::new(vec![revision.clone()])?;
assert_eq!(decide_object_revision_v1(&revision, &revision, &chain)?, RevisionDecisionV1::AlreadyApplied);
```
