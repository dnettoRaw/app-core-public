# appcore-types

Release candidate **`appcore-types 1.0.4-rc.1`**. Sélectionner explicitement cette RC ; elle ne remplace pas les versions stables antérieures et ne certifie pas un déploiement. Les dépendances AppCore internes, lorsqu’elles existent, sont fixées sur cette série RC.

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** identifiants validés, identity et trace partagés par les
contrats.

**Dépendances internes :** `appcore-contracts`.

**API principale :** IDs application, node, tenant, cluster, Core, instance,
command, event, query, state et capability; `RuntimeIdentity`, `CoreIdentity`,
policies/status de version, `TraceContext`, `RuntimeError`,
`RuntimeResult`.

Utiliser ces types au lieu de strings non validées aux frontières. Ne pas y
placer état d'implémentation, I/O ou comportement provider.

**Maturité :** surface fondamentale RC stable.
