# appcore-args

Release candidate **`appcore-args 1.0.3-rc.1`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.

`RawArgs::from_env` validates each word before retaining it and stops at the
first invalid argument. It does not collect the remaining argv into a crate-owned
buffer before enforcing count and byte limits.

```text
       #######
    ###       ###
   ##   ## ##   ##
        ## ##
   ##   ## ##   ##
     ###########
```

Author: [dnettoRaw](https://github.com/dnettoRaw)

[Português](README.pt.md) | [Français](README.fr.md)

Dependency-free, cross-platform command-line parsing, help, validation and
shell completion primitives for `AppCore` executables.

The crate has independent `SemVer`, no dependencies and can be consumed by any
Rust executable without the `AppCore` Runtime.

`appcore-args` provides nested commands, inherited options, typed and bounded
arguments, deterministic errors, generated help, completion candidates and
dynamic completion scripts for Bash, Zsh, Fish and PowerShell. It executes no
Runtime behavior and uses no unsafe code.

```rust
use appcore_args::{ArgumentSpec, CliParser, CliSpec, CommandSpec, OptionSpec, RawArgs};

let spec = CliSpec::new("demo")
    .option(OptionSpec::flag("verbose").short('v'))
    .command(CommandSpec::new("run").argument(
        ArgumentSpec::new("mode").required(true).possible_value("safe"),
    ));
let parsed = CliParser::new(&spec)
    .parse(&RawArgs::parse(["run", "safe", "-v"])? )?;

assert_eq!(parsed.command_path(), &["run"]);
assert!(parsed.has_flag("verbose"));
# Ok::<(), Box<dyn std::error::Error>>(())
```

See the [English guide](wiki/guide.en.md), the
[minimal example](wiki/examples/basic.en.md) and the
[intermediate example](wiki/examples/intermediate.en.md). API documentation is available on
[docs.rs](https://docs.rs/appcore-args).

License: MIT.

## Stable documentation

Stable ID: **ACR-001**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-001). This permanent ID
remains valid if the wiki page moves.

## Declarative configuration and native paths

Unpublished source additions: `OptionSpec::env`, `default_value`, `value_kind`
and `exclusive_group`. Precedence is argv, declared environment, then one
fallback value. Flags from env/default accept only `true` or `false`; false
means absent. Required options, conflicts and requirements are checked after
resolution. A group accepts at most one member and `requires("source")` can
require a member of that group. Environment/default values are bounded together
with argv. Terminal help skips environment lookup, but still validates the spec.
Errors from environment/default validation and parsed option Debug hide values.
Help/completion never read environment values.

Use `ValueKind::Path` for a nonempty native path or `ValueKind::SocketAddr` for a
numeric IP/port (including bracketed IPv6, without DNS). `ValueType` retains its
existing enum. `RawArgs` remains UTF-8-only; `RawOsArgs::from_env` and
`CliParser::parse_os` preserve native paths and passthrough. Non-UTF-8 text,
option names and addresses fail. Paths are not canonicalized or checked for
existence. `option_path` returns `PathBuf`; `option_socket_addr` returns a typed
address. `parse_with_env` and `parse_os_with_env` allow deterministic injection.

```rust
use appcore_args::{CliSpec, CliParser, OptionSpec, RawOsArgs, ValueKind};
let spec = CliSpec::new("node")
    .option(OptionSpec::value("data-dir").env("NODE_DATA_DIR")
        .default_value("./data").value_kind(ValueKind::Path))
    .option(OptionSpec::value("bind").env("NODE_BIND")
        .default_value("127.0.0.1:9000").value_kind(ValueKind::SocketAddr))
    .option(OptionSpec::value("descriptor").exclusive_group("source"))
    .option(OptionSpec::value("catalog").exclusive_group("source"))
    .option(OptionSpec::flag("serve").requires("source"));
let raw = RawOsArgs::parse([std::ffi::OsString::from("--bind=[::1]:9000")])?;
let parsed = CliParser::new(&spec).parse_os_with_env(&raw, |_| None)?;
assert!(parsed.option_path("data-dir").is_some());
assert_eq!(parsed.option_socket_addr("bind")?.unwrap().port(), 9000);
# Ok::<(), appcore_args::CliError>(())
```
