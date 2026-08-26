# appcore-core

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** lifecycle, enregistrement, dispatch, state, audit et
idempotence génériques dans le processus.

**Dépendances internes :** `appcore-contracts`, `appcore-types`.

**API principale :** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, registries et buses command/event, enveloppes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log/journal,
idempotence mémoire/fichier, state et decision engines, clock, redaction et
`AppPlugin` de compatibilité.

Les valeurs clonées de `RuntimeController` partagent lifecycle, idempotence et
commandes en cours. Le command bus immuable possède les handlers via `Arc`. Les
handlers indépendants peuvent s'exécuter en parallèle, tandis qu'une clé
idempotente n'admet qu'une exécution. Demandez le shutdown avant le drainage
borné ; les nouvelles commandes sont alors rejetées sans course avec la
transition lifecycle.

Les nouvelles applications utilisent les re-exports de
`appcore_bin::application`; elles n'assemblent pas le core. Garder I/O adapters
et comportement domaine hors de ce crate.

**Maturité :** surface low-level RC stable; builder/plugin restent de
compatibilité, manifest-first est préféré.
