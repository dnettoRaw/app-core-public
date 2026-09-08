# appcore-sdk

The `runtime` benchmark measures default facade construction, empty
`App::prepare`, and preparation with 32 registered commands. Run
`cargo bench -p appcore-sdk --bench runtime`; set `APPCORE_BENCH_CASE` to
`construct_defaults`, `prepare_empty` or `prepare_32_commands` to isolate one
case. The benchmark is also discoverable by `appcore-dev bench`. It measures
construction, validation and teardown, not HTTP startup or cluster operation.
Building with `--features full` changes the linked feature profile; it does not
exercise every optional capability.

[Português](README.pt.md) | [Français](README.fr.md)

`appcore-sdk` is the documented application facade for AppCore. It keeps a
new application's dependency surface small and delegates infrastructure to the
crates that own it. It replaces the retired `appcore-bin`; it does not preserve
the old Runtime host or CLI.

```rust
use appcore_sdk::prelude::*;

fn main() -> AppResult<()> {
    appcore_sdk::run("hello-world", |app| {
        app.log("Hello, world!");
        Ok(())
    })
}
```

Implement `Application` when registering commands, queries and tasks for an
explicit deployment. The base facade only requires contracts and the Runtime
core plus bounded logging. `App::prepare` executes every registration hook and
returns immutable Core registries, a frozen query router and bounded task
definitions without starting a host. Enable `api` for HTTP queries, `scheduler` for scheduled work,
`deployment` for provider-resolved bindings, or `storage`, `sync`, `ai` and
`filemaker` only when those capabilities are required. Provider composition,
listeners and process lifecycle remain application/deployment concerns rather
than library responsibilities.

## Executable examples

Examples are grouped by progression under `examples/`: `01-basics`,
`02-manifests`, `03-application`, `04-storage`, `05-sync`, `06-ai` and
`07-filemaker`, `08-logging` and `09-api`. Start each with
`cargo run -p appcore-sdk --example NAME`; the
capability examples require the corresponding SDK feature. They cover every
public SDK path without pretending that deployment process composition is an
SDK capability.

Use `App::logging(LoggerConfig)` to select terminal, bounded JSONL file, both,
disabled, or crash-only output. File names, per-file bytes, active rotations
and the optional `YYYY/MM` archive are explicit. Crash-only output is written
with `App::dump_crash_log`; it creates no file during normal execution. See
`08-configured-logging`.

## Stable documentation

Stable ID: **ACR-028**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-028). This permanent ID
remains valid if the wiki page moves.

Existing applications should follow the crate-owned
[migration guide](wiki/migration.en.md).
