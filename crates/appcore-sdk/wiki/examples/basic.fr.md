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
