# Intermediate example: files, archive, and filters

This scenario writes normal operations to terminal and JSONL, raises detail
only for `sync`, and keeps the active directory small.

```rust
use appcore_log::{
    FileArchiveConfig, FileSinkConfig, LogOutputMode, LogPolicy, LoggerConfig,
    Verbosity, LOG_SIZE_8_MIB,
};

fn main() -> Result<(), appcore_log::LogConfigError> {
    // The application stays at V4; only sync accepts diagnostics through V8.
    let mut policy = LogPolicy::new(Verbosity::V4);
    policy.set_component("sync", Verbosity::V8);

    let logger = LoggerConfig {
        policy,
        output: LogOutputMode::TerminalAndFile,
        file: Some(FileSinkConfig {
            // The name is free; the application prepares the directory.
            path: "logs/application.jsonl".into(),
            max_bytes: LOG_SIZE_8_MIB,
            sync_each_write: false,
            retention: 2,
            archive: Some(FileArchiveConfig {
                directory: "logs/archive".into(),
                max_files: 120,
            }),
        }),
        ..LoggerConfig::default()
    }
    .build()?;

    let application = logger.dispatcher().event(0, "application");
    let sync = logger.dispatcher().event(1, "sync.transport");

    application.info("application ready");

    // V7 is visible because sync.transport inherits the sync policy.
    sync.verbosity(7).debug("replication batch sent");

    // The override is immutable; this event uses the builder's V4 again.
    sync.warn("peer response was delayed");

    let totals = logger.dispatcher().stats();
    let destinations = logger.dispatcher().sink_stats();

    assert_eq!(totals.sink_failures, 0);
    assert_eq!(destinations.len(), 2);

    Ok(())
}
```

After rotation, the layout resembles:

```text
logs/
├── application.jsonl
├── application.jsonl.1
├── application.jsonl.2
└── archive/
    └── 2026/
        └── 09/
            └── application-00000001756944000000-0000.jsonl
```

JSONL contains only events that passed filtering and sanitization. For paths
and secrets, construct `LogEvent` with `.path(...)` and `.secret(...)`. For
Sensitive content, use the separate `sensitive_diagnostics` example; never
send it to ordinary JSONL.

Run the maintained file example:

```shell
cargo run -p appcore-log --example file_logging
```

The result is `target/appcore-log-example/application.jsonl`. Git already
ignores `target`, and Cargo clean can remove it.

When durable file I/O must not block the producer, use the explicit bounded
wrapper example:

```shell
cargo run -p appcore-log --example async_file
```

It prints the absolute path of `target/appcore-log-example/async.jsonl`, limits
the queue by events and bytes, and calls `shutdown` before exit. Saturation is
reported rather than waiting or growing memory.

Return to the [guide](../guide.en.md).
