# Minimal `appcore-args` CLI

Author: [dnettoRaw](https://github.com/dnettoRaw)

[Português](basic.pt.md) | [Français](basic.fr.md) |
[Intermediate example](intermediate.en.md) | [Guide](../guide.en.md)

This is the smallest useful executable: one required positional argument,
bounded process input and a typed parse result.

```rust
use appcore_args::{ArgumentSpec, CliError, CliParser, CliSpec, RawArgs};

fn main() -> Result<(), CliError> {
    let spec = CliSpec::new("hello")
        .about("Print a greeting.")
        .argument(ArgumentSpec::new("name").required(true));
    let raw = RawArgs::from_env()?;
    let parsed = CliParser::new(&spec).parse(&raw)?;

    println!("Hello, {}!", parsed.positionals()[0]);
    Ok(())
}
```

Run it with `cargo run -- Ana`. `RawArgs::from_env` rejects non-UTF-8, NUL,
oversized words and excessive argument counts before command parsing begins.
