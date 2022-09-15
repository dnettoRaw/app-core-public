# CLI minimale avec `appcore-args`

Auteur : [dnettoRaw](https://github.com/dnettoRaw)

[English](basic.en.md) | [Português](basic.pt.md) |
[Exemple intermediaire](intermediate.fr.md) | [Guide](../guide.fr.md)

Voici le plus petit executable utile : un argument positionnel obligatoire,
une entree de processus bornee et un resultat d'analyse type.

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

Executez avec `cargo run -- Ana`. `RawArgs::from_env` rejette l'UTF-8 invalide,
les NUL, les mots trop grands et les listes d'arguments excessives avant
l'analyse.
