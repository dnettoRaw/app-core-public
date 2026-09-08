# appcore-log

[Português](README.pt.md) | [Français](README.fr.md)

`appcore-log` provides structured, bounded, and security-first operational
logging for AppCore. It has no global logger or hidden queue and creates no
background worker unless the caller explicitly selects `AsyncSink`. The caller
chooses filtering, destinations, retention, and durability.

## Quick start

```rust
use appcore_log::{LogOutputMode, LoggerConfig};

fn main() -> Result<(), appcore_log::LogConfigError> {
    let logger = LoggerConfig {
        output: LogOutputMode::Terminal,
        ..LoggerConfig::default()
    }
    .build()?;

    let log = logger.dispatcher().event(0, "application");

    log.info("application started");
    log.warn("connection is slow");

    Ok(())
}
```

Applications using `appcore-sdk` configure the same logger with `App::logging`
and emit through `app.log(...)` or `app.logger()`.

## Output modes

| Mode | Normal execution | File required |
|---|---|---|
| `Terminal` | Human-readable terminal output | No |
| `File` | Structured bounded JSONL | Yes |
| `TerminalAndFile` | Terminal and JSONL | Yes |
| `Disabled` | No sink work or message conversion | No |
| `CrashOnly` | Bounded sanitized memory ring | Yes, created only by `dump_crash` |

`CrashOnly` does not install a panic handler. Call `dump_crash` once from the
application's controlled crash boundary.

## Bounded file output

```rust
use appcore_log::{
    FileArchiveConfig, FileSinkConfig, LogOutputMode, LoggerConfig,
    LOG_SIZE_8_MIB,
};

fn main() -> Result<(), appcore_log::LogConfigError> {
    let logger = LoggerConfig {
        output: LogOutputMode::TerminalAndFile,
        file: Some(FileSinkConfig {
            path: "logs/runtime.jsonl".into(),
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

    logger.dispatcher().event(0, "application").info("ready");

    Ok(())
}
```

The active directory contains `runtime.jsonl`, `runtime.jsonl.1`, and
`runtime.jsonl.2`. Older rotations move to `archive/YYYY/MM` until the complete
archive reaches `max_files`. The file name is application-defined.
Create the active file's parent directory during application setup; archive
year/month directories are created during rotation. File and archive
destinations reject symbolic links instead of following them.

Set `sync_each_write: true` when every event must be flushed to storage before
returning. `false` avoids that syscall and favors throughput. Common limits are
available from `LOG_SIZE_1_MIB` through `LOG_SIZE_64_MIB`; custom nonzero `u64`
values remain valid.

## Severity, verbosity, and components

Severity describes impact, from `Trace` to `Critical`. Verbosity describes
detail, from V1 to V9. A V4 policy accepts V1 through V4; it does not mean
"severity four". Component overrides are hierarchical, so an override for
`sync` also applies to `sync.transport` unless a more specific one exists.

## Security boundary

Safe and Diagnostic policies redact typed secrets and alias typed paths before
ordinary sinks receive an event. Use `LogEvent::secret` and `LogEvent::path`;
never put credentials in free-form messages. Full paths require a separate
explicit policy decision.

Sensitive diagnostics require `Sensitivity::Sensitive` and an explicitly
wired `SensitiveDntSink`. They are authenticated and encrypted as DNT and never
fall back to terminal, JSONL, or ordinary memory sinks. Secret fields remain
redacted even there.

## Limits and failure behavior

- Event text: 4 KiB per text or identity field.
- Structured fields: 32 per event.
- Field keys: 128 bytes; field values: 4 KiB.
- Active rotations: at most 32.
- Archived files: from 1 to 10,000.
- Rings: always bounded by both event count and estimated retained bytes.
- Sink failures: counted by the dispatcher and never logged recursively.

Use `stats()` for aggregate filtering/failure counters and `sink_stats()` for
per-destination failures. Use `FixedLogClock` for deterministic tests.

See the [English guide](wiki/guide.en.md) and the executable examples in
[`examples/`](examples/).

Before allocating a JSONL payload, the file sink counts serialized bytes,
including escaping and the newline, against `max_bytes`. Oversized records
return `Capacity` before rotation or writing. Accepted records use an exact-size
reservation and a second serialization pass; allocator overhead, caller-owned
events and filesystem memory are outside this payload budget.

Each `FileSink` keeps one append handle open and tracks the active file size
under its existing mutex. It closes the handle before rotation and reopens it
if the path is removed, replaced, truncated, or appended by another writer.
There is still no hidden queue or background flush worker.

For an explicitly asynchronous boundary, wrap one sink in `AsyncSink`. Its
non-blocking `emit` is bounded by both `AsyncSinkConfig::max_events` and
`max_bytes`; saturation returns `Capacity` and is counted. `flush` waits for
previously admitted events and `shutdown` drains and joins the single 256 KiB
worker. The wrapped sink must itself complete: arbitrary blocking I/O cannot be
forcibly terminated safely. Dropping without explicit shutdown never waits for
that I/O. The queue is opt-in and never changes `LoggerConfig` defaults.

## Benchmark

Run the ten hardware-aware workloads with:

```shell
cargo run -p appcore-dev -- bench --name appcore-log \
  --output target/appcore-log-benchmark.json
```

The report records timing distributions, CPU time, peak/retained RSS, and host
CPU, RAM, GPU, OS, and filesystem context. Compare compatible reports with
`appcore-dev bench compare`.

The concurrent JSONL cases measure 64 events per batch from four producers,
with and without per-event sync. The slow-sink case serializes four events
with a simulated 1 ms wait each; it is not a disk measurement. See the guide
for timing boundaries.

## Stable documentation

Stable ID: **ACR-027**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-027). This permanent ID
remains valid if the wiki page moves.
