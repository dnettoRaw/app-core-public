# appcore-args

```text
       #######
    ###       ###
   ##   ## ##   ##
        ## ##
   ##   ## ##   ##
     ###########
```

Auteur : [dnettoRaw](https://github.com/dnettoRaw)

[English](README.en.md) | [Português](README.pt.md)

Primitives sans dépendances et multiplateformes pour le parsing de ligne de
commande, l'aide, la validation et la complétion shell des exécutables AppCore.

Le crate possède un SemVer indépendant, n'a aucune dépendance et peut être
utilisé par tout exécutable Rust sans le Runtime AppCore.

`appcore-args` fournit commandes imbriquées, options héritées, arguments typés
et bornés, erreurs déterministes, aide générée, candidats de complétion et
scripts dynamiques pour Bash, Zsh, Fish et PowerShell. Il n'exécute aucun
comportement Runtime et n'utilise aucun code `unsafe`.

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

Consultez le [guide français](wiki/guide.fr.md) ainsi que les exemples
[minimal](wiki/examples/basic.fr.md) et
[intermediaire](wiki/examples/intermediate.fr.md). La documentation de l'API
est disponible sur [docs.rs](https://docs.rs/appcore-args).

Licence : MIT.
