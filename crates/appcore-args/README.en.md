# appcore-args

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
shell completion primitives for AppCore executables.

The crate has independent SemVer, no dependencies and can be consumed by any
Rust executable without the AppCore Runtime.

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
