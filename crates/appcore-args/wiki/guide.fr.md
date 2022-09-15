# Guide appcore-args

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
