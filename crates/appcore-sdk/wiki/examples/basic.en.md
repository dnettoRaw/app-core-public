# Basic SDK example

```rust
use appcore_sdk::prelude::*;

fn main() -> AppResult<()> {
    appcore_sdk::run("hello-world", |app| {
        app.log("Hello, world!");
        Ok(())
    })
}
```

This uses one dependency and performs no I/O besides the explicit log call.
