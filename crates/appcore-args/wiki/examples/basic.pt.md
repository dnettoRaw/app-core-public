# CLI minima com `appcore-args`

Autor: [dnettoRaw](https://github.com/dnettoRaw)

[English](basic.en.md) | [Français](basic.fr.md) |
[Exemplo intermediario](intermediate.pt.md) | [Guia](../guide.pt.md)

Este e o menor executavel util: um argumento posicional obrigatorio, entrada do
processo limitada e um resultado de parse tipado.

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

Execute com `cargo run -- Ana`. `RawArgs::from_env` rejeita UTF-8 invalido,
NUL, palavras grandes demais e excesso de argumentos antes do parse.
