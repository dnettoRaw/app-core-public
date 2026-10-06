# appcore-args Guide

`RawArgs::from_env` validates each word before retaining it and stops at the
first invalid argument. It does not collect the remaining argv into an AppCore
buffer before enforcing count and byte limits.

Author: [dnettoRaw](https://github.com/dnettoRaw)

[Português](guide.pt.md) | [Français](guide.fr.md) |
[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

## Ownership

The crate owns command specifications, bounded argument ingestion,
deterministic parsing, generated help, completion candidates and dynamic shell
integration. Consumers own command execution and all Runtime behavior.

This is a standalone, independently versioned crate. Its public API must not
import contracts or types from any other AppCore crate.

## Command Model

- `CliSpec` and `CommandSpec` define nested commands, aliases, inherited
  options, positional arguments and required subcommands.
- `OptionSpec` defines long and short names, forbidden, required or optional
  values, repetition, requirements and conflicts.
- Terminal options such as `--help` may bypass required-input checks.
- `ArgumentSpec` defines fixed or final variadic positional arguments.
- `ValueType` validates text, booleans, signed integers and unsigned integers.

Every specification is validated before parsing, help or completion. Invalid
names, duplicate aliases, inherited option collisions, unknown relationships
and ambiguous positional layouts fail closed.

## Input Boundary

`RawArgs::from_env` rejects non-UTF-8 input instead of converting it with loss.
The default limits are 1,024 words, 64 KiB per word and 1 MiB in total. Custom
limits are available through `RawArgs::parse_with_limits`. NUL bytes are always
rejected.

The parser accepts `--name value`, `--name=value`, grouped flags such as `-av`,
attached short values such as `-oresult` or `-o=result`, signed negative
positionals and passthrough after `--`. Optional values deliberately accept
only `--name=value` or an attached short value, so the next positional is never
consumed ambiguously. A consumer may opt into a detached optional value with
`detached_optional_value(true)`; use a restrictive type such as `Bool` so only
a valid next word is consumed.

Unknown commands, long options and enumerated values include a close suggestion
when one exists. Suggestion work is bounded to 128-byte inputs and candidates;
larger values still return their typed error without similarity analysis.

## Help And Completion

`HelpRenderer` and `CompletionEngine` consume the same validated specification
as the parser. Hidden entries are omitted, consumed non-repeatable options are
not suggested, and declared possible values become completion candidates.

`render_dynamic_completion_script` supports Bash, Zsh, Fish and PowerShell.
Executable and completion-command tokens are restricted before interpolation.
When no structural candidate exists, generated integrations preserve the
shell's native file completion.

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
