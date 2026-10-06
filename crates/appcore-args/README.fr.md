# appcore-args

`RawArgs::from_env` valide chaque mot avant de le conserver et s'arrête au
premier argument invalide. Il ne collecte pas le reste de argv dans un tampon
AppCore avant d'appliquer les limites de nombre et d'octets.

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

## Documentation stable

Identifiant stable : **ACR-001**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-001). Cet identifiant
permanent reste valable si la page du wiki est déplacée.

## Configuration déclarative et chemins natifs

Ajouts source non publiés : `OptionSpec::env`, `default_value`, `value_kind` et
`exclusive_group`. Priorité : argv, environnement déclaré, puis une valeur par
défaut. Les flags env/default acceptent seulement `true` ou `false` ; false
signifie absent. Options requises, conflits et exigences sont vérifiés après
résolution. Un groupe accepte au plus un membre ; `requires("source")` peut
exiger un membre du groupe. Env/default partagent les budgets argv. Help terminal
ne consulte pas l'environnement mais valide la spec. Erreurs env/default et
Debug des options masquent les valeurs. Help/completion ne lisent pas les valeurs.

`ValueKind::Path` accepte un chemin natif non vide et `ValueKind::SocketAddr`
un IP/port numérique, IPv6 entre crochets compris, sans DNS. `ValueType` conserve
son enum actuel. `RawArgs` reste UTF-8 ; `RawOsArgs::from_env` et
`CliParser::parse_os` préservent chemins et passthrough natifs. Texte, noms et
adresses non UTF-8 échouent. Les chemins ne sont ni canonicalisés ni vérifiés
sur disque. `option_path` retourne `PathBuf` ; `option_socket_addr` une adresse
typée. `parse_with_env` et `parse_os_with_env` permettent l'injection déterministe.

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
