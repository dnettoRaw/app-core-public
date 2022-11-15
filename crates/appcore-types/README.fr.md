# appcore-types

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
