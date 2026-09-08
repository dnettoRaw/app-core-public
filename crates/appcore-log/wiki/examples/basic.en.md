# Basic example: safe terminal logging

This example creates a local logger, keeps a builder for the application
component, and emits three severities. The default policy is Safe at V4.

```rust
use appcore_log::{LogOutputMode, LoggerConfig};

fn main() -> Result<(), appcore_log::LogConfigError> {
    // Configuration belongs to the application and fails explicitly if invalid.
    let logger = LoggerConfig {
        output: LogOutputMode::Terminal,
        ..LoggerConfig::default()
    }
    .build()?;

    // Reusing the builder avoids repeating the component for every call.
    let log = logger.dispatcher().event(0, "application");

    log.info("application started");
    log.warn("connection is slower than expected");
    log.error("document could not be saved");

    Ok(())
}
```

Approximate output:

```text
[Info][application][V4] application started
[Warn][application][V4] connection is slower than expected
[Error][application][V4] document could not be saved
```

Timestamp `0` keeps the example deterministic; a real application can use
`SystemLogClock` with `event_now`. No thread or global state is created.

Run the maintained crate example:

```shell
cargo run -p appcore-log --example basic
```

Next: [intermediate example](intermediate.en.md).
