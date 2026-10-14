# Exemple SDK de base

```rust
use appcore_sdk::prelude::*;

fn main() -> AppResult<()> {
    appcore_sdk::run("hello-world", |app| {
        app.log("Bonjour, monde !");
        Ok(())
    })
}
```

Cet exemple utilise une dépendance et ne fait aucun I/O hors du log explicite.

Exemple supplémentaire de contrat borné ; exécutez dans un contexte retournant Result avec les dépendances explicitement choisies.

```rust
use appcore_sdk::sync::{ObjectRevisionV1, RevisionChainV1, decide_object_revision_v1, RevisionDecisionV1};
let revision = ObjectRevisionV1 { object_id: "opaque-a".into(), revision_id: "a".into(), base_revision_id: None, event_id: "event-a".into() };
let chain = RevisionChainV1::new(vec![revision.clone()])?;
assert_eq!(decide_object_revision_v1(&revision, &revision, &chain)?, RevisionDecisionV1::AlreadyApplied);
```
