# AppCore Log guide

This guide moves from terminal output to rotation, crash-only capture, and
encrypted diagnostics. A logger belongs to its owner: no global configuration,
thread, or queue is created implicitly.

## 1. Choose a destination

`LoggerConfig::default()` selects safe V4 terminal output.

| `LogOutputMode` | Behavior |
|---|---|
| `Terminal` | Writes readable text to stdout. |
| `File` | Writes one JSON object per line. |
| `TerminalAndFile` | Sends the sanitized event to both destinations. |
| `Disabled` | Returns before message conversion or sink work. |
| `CrashOnly` | Retains a bounded ring and normally creates no file. |

File-based modes require `file: Some(...)`. Invalid configuration returns
`LogConfigError`; it never silently falls back.

```rust
use appcore_log::{LogOutputMode, LoggerConfig};

let logger = LoggerConfig {
    output: LogOutputMode::Terminal,
    ..LoggerConfig::default()
}
.build()
.expect("valid configuration");
```

## 2. Reuse a component builder

```rust
use appcore_log::LoggerConfig;

let logger = LoggerConfig::default()
    .build()
    .expect("valid configuration");
let log = logger.dispatcher().event(0, "sync");

log.info("replication started");
log.warn("peer temporarily unavailable");
log.error("checkpoint was not persisted");
```

SDK applications configure once with `App::logging(config)` and use
`app.logger()`.

## 3. Bound files and retention

| `FileSinkConfig` field | Purpose |
|---|---|
| `path` | Active directory and file name. |
| `max_bytes` | Nonzero maximum size of each JSONL file. |
| `sync_each_write` | Durably flushes every event when true. |
| `retention` | Rotations kept beside the active file, from 0 to 32. |
| `archive` | Optional destination for rotations leaving active retention. |

`FileArchiveConfig.directory` receives `YYYY/MM` folders. `max_files` bounds
the complete archive from 1 to 10,000 files.

```rust
use appcore_log::{
    FileArchiveConfig, FileSinkConfig, LogOutputMode, LoggerConfig,
    LOG_SIZE_8_MIB,
};

let logger = LoggerConfig {
    output: LogOutputMode::File,
    file: Some(FileSinkConfig {
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
.build()
.expect("valid configuration");
```

Use `LOG_SIZE_1_MIB` through `LOG_SIZE_64_MIB`, or any custom nonzero `u64`.
Set `sync_each_write` to false for throughput and true when per-event storage
durability is worth the I/O cost.

The application must create the active file's parent directory before the
first event. The sink creates archive `YYYY/MM` folders when needed and rejects
symbolic-link destinations.

JSONL serialization counts bytes without a payload allocation before checking
`max_bytes`, including escaping and the newline. Rejected records do not rotate
or modify files. Accepted records reserve the exact payload size and serialize
again, trading a second traversal for bounded scratch allocation. This is not
a process RSS cap; allocator overhead and caller-owned events remain separate.

## 4. Separate severity from verbosity

Severity is impact: `Trace`, `Debug`, `Info`, `Warn`, `Error`, or `Critical`.
Verbosity is required detail: V1–V3 cover important failure information, V4–V6
normal operation, and V7–V9 technical diagnostics. A V4 policy accepts V1–V4.

| Range | Typical use |
|---|---|
| V1–V3 | Critical failures, important errors, and relevant warnings. |
| V4–V6 | Essential events, normal operation, and flow details. |
| V7–V9 | Technical debug, I/O, timing, and deep diagnostics. |

```rust
use appcore_log::{LogPolicy, Verbosity};

let mut policy = LogPolicy::new(Verbosity::V4);
policy.set_component("sync", Verbosity::V8);
```

The `sync` override also applies to `sync.transport`; a more specific override
wins. `log.verbosity(7).debug(...)` changes only that event.

## 5. Protect secrets and paths

Use `LogEvent::field` for ordinary data, `LogEvent::path` for local paths, and
`LogEvent::secret` for credentials. Safe and Diagnostic output sanitize before
terminal, file, or ring delivery. `PathAliases` can produce `<APP_ROOT>` and
similar aliases; unknown paths become `<LOCAL_PATH>`.

```rust
use appcore_log::{LogEvent, Severity, Verbosity};

let event = LogEvent::new(0, Severity::Info, Verbosity::V4, "storage", "opened")
    .path("file", "/srv/app/data.json")
    .secret("token", "must not appear");
```

Do not embed secrets or unrestricted paths in free-form messages. Typed fields
are the enforceable boundary. Diagnostic sensitivity keeps the same protection.
Sensitive output requires an explicit `SensitiveDntSink` and encrypted DNT; the
dispatcher refuses ordinary sinks and never falls back to plaintext.

## 6. Capture only for a crash

Crash-only mode bounds memory with `crash_events` and `crash_bytes`. It does not
install a panic handler. Call `dump_crash` once from the application's
controlled crash boundary; the configured file is absent before that call.

```rust
use appcore_log::{FileSinkConfig, LogOutputMode, LoggerConfig, LOG_SIZE_2_MIB};

let logger = LoggerConfig {
    output: LogOutputMode::CrashOnly,
    file: Some(FileSinkConfig {
        path: "logs/crash.jsonl".into(),
        max_bytes: LOG_SIZE_2_MIB,
        sync_each_write: true,
        retention: 1,
        archive: None,
    }),
    crash_events: 128,
    crash_bytes: 512 * 1024,
    ..LoggerConfig::default()
}
.build()
.expect("valid configuration");

// Execute once from the controlled crash boundary.
let _written = logger.dump_crash().expect("crash dump written");
```

## 7. Observe failures and test

- `stats()` reports filtered, invalid, failed, and dropped events.
- `sink_stats()` separates failures by destination.
- `FixedLogClock` supplies deterministic timestamps.
- Public event limits are 4 KiB per text/identity, 32 fields, 128 bytes per key,
  and 4 KiB per value.

Sink failures are counted without recursive logging. Rings require both event
and byte ceilings.

Continue with the [basic example](examples/basic.en.md) or the
[intermediate example](examples/intermediate.en.md).

## 8. Measure on real hardware

The native benchmark covers disabled/filtered emission, sanitization, bounded
ring storage, buffered/durable JSONL, and archive rotation:

```shell
cargo run -p appcore-dev -- bench --name appcore-log \
  --output target/appcore-log-benchmark.json
```

The JSON includes p50/p95, CPU, peak/retained RSS, toolchain, and hardware.
Keep a baseline and use `appcore-dev bench compare` before accepting an
optimization.

The synchronous file sink reuses one append handle. Its mutex covers byte
counting, replacement detection, rotation, writing, and optional sync. It
reopens after external removal, replacement, truncation, or append; it does not
create an asynchronous queue or change the configured durability policy.

`AsyncSink` is the explicit asynchronous alternative. Configure both retained
event and estimated-byte ceilings, pass it as a `LogSink`, and call `flush` or
`shutdown` at the owning lifecycle boundary. Admission never waits and reports
`Capacity` on saturation. The worker isolates sink panic, exposes delivery,
failure and rejection counters, and uses a 256 KiB stack. Shutdown can only be
bounded when the wrapped sink's own I/O is bounded; drop does not wait.

The `jsonl_concurrent_buffered_batch_64` and
`jsonl_concurrent_synced_batch_64` cases use four producers sharing one
dispatcher and file sink, with 16 events each per iteration. Timing includes
event construction, sanitization, file I/O and two barrier rendezvous per
batch; worker creation/join and directory setup/removal are excluded.
`serialized_slow_sink_batch_4` instead emits one event per producer into a
mutex-serialized sink that sleeps 1 ms per event. This models blocking, not
physical disk latency. Reported times are per batch, not per event or latency
percentiles of individual producers. RSS includes worker stacks. These cases
measure the synchronous baseline; unit tests, rather than timing, prove
active-delivery queue bounds and explicit flush/shutdown behavior. They do not
prove crash durability.
