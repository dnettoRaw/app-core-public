# appcore-core

**Responsabilité :** lifecycle, enregistrement, dispatch, state, audit et
idempotence génériques dans le processus.

**Dépendances internes :** `appcore-contracts`, `appcore-types`.

**API principale :** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, registries et buses command/event, enveloppes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log/journal,
idempotence mémoire/fichier, state et decision engines, clock, redaction et
`AppPlugin` de compatibilité.

Les nouvelles applications utilisent les re-exports de
`appcore_bin::application`; elles n'assemblent pas le core. Garder I/O adapters
et comportement domaine hors de ce crate.

**Maturité :** surface low-level RC stable; builder/plugin restent de
compatibilité, manifest-first est préféré.
