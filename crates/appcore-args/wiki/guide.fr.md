# Guide appcore-args

Release candidate **`appcore-args 1.0.3-rc.1`**. Sélectionner explicitement cette RC ; elle ne remplace pas les versions stables antérieures et ne certifie pas un déploiement. Les dépendances AppCore internes, lorsqu’elles existent, sont fixées sur cette série RC.

`RawArgs::from_env` valide chaque mot avant de le conserver et s'arrête au
premier argument invalide. Il ne collecte pas le reste de argv dans un tampon
AppCore avant d'appliquer les limites de nombre et d'octets.

Auteur : [dnettoRaw](https://github.com/dnettoRaw)

[English](guide.en.md) | [Português](guide.pt.md) |
[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

## Responsabilité

Le crate possède les spécifications de commandes, la lecture bornée des
arguments, le parsing déterministe, l'aide générée, les candidats de
complétion et l'intégration dynamique aux shells. Les consommateurs possèdent
l'exécution et tout comportement Runtime.

Il s'agit d'un crate autonome avec version indépendante. Son API publique ne
doit importer aucun contrat ou type d'un autre crate AppCore.

## Modèle De Commandes

- `CliSpec` et `CommandSpec` définissent commandes imbriquées, alias, options
  héritées, positionnels et sous-commandes obligatoires.
- `OptionSpec` définit noms longs et courts, valeurs interdites, obligatoires
  ou facultatives, répétition, exigences et conflits.
- Les options terminales comme `--help` peuvent ignorer les contrôles des
  entrées obligatoires.
- `ArgumentSpec` définit des positionnels fixes ou un dernier variadique.
- `ValueType` valide texte, booléens et entiers signés ou non signés.

Chaque spécification est validée avant parsing, aide ou complétion. Noms
invalides, alias dupliqués, collisions d'options héritées, relations inconnues
et dispositions positionnelles ambiguës échouent de manière fermée.

## Frontière D'Entrée

`RawArgs::from_env` refuse les entrées non UTF-8 sans conversion avec perte.
Les limites par défaut sont 1 024 mots, 64 Kio par mot et 1 Mio au total. Des
limites personnalisées sont disponibles avec `RawArgs::parse_with_limits`. Les
octets NUL sont toujours refusés.

Le parser accepte `--name value`, `--name=value`, les groupes comme `-av`, les
valeurs courtes attachées comme `-oresult` ou `-o=result`, les positionnels
signés négatifs et le passthrough après `--`. Les valeurs facultatives acceptent
uniquement `--name=value` ou une valeur courte attachée afin de ne jamais
consommer le positionnel suivant de façon ambiguë. Un consommateur peut
autoriser une valeur facultative séparée avec `detached_optional_value(true)`;
utilisez un type restrictif comme `Bool` afin de ne consommer que le mot suivant
valide.

Les commandes, options longues et valeurs énumérées inconnues incluent une
suggestion proche quand elle existe. Le calcul est limité aux entrées et
candidats de 128 octets; les valeurs plus grandes renvoient toujours leur
erreur typée sans analyse de similarité.

## Aide Et Complétion

`HelpRenderer` et `CompletionEngine` consomment la même spécification validée
que le parser. Les entrées cachées sont omises, les options non répétables déjà
utilisées ne sont pas suggérées et les valeurs déclarées deviennent des
candidats.

`render_dynamic_completion_script` supporte Bash, Zsh, Fish et PowerShell. Les
tokens de l'exécutable et de la commande sont restreints avant interpolation.
Sans candidat structurel, les intégrations préservent la complétion native des
fichiers du shell.

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
