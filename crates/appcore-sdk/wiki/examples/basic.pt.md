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
