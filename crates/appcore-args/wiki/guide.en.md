# appcore-args Guide

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
