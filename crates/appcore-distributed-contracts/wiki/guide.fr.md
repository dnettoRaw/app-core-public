# appcore-distributed-contracts

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** contrats wire/provider versionnés control plane et peer
RPC.

**Dépendances internes :** `appcore-contracts`, `appcore-types`.

**API principale :** constantes et paths protocole, registration, presence,
heartbeat, peer directory, leases de compatibilité, leases par service,
leadership decisions et traits; paths peer, enveloppes, réponses, erreurs, call
kinds, advertisement DTOs, client executor et métadonnées de transport pour
content-envelope opaque.

Les implémentations appartiennent aux crates control plane ou peer. Ne pas
ajouter client HTTP, filesystem, tokens ou règles capability produit.

La serialisation wire opaque-content et Peer RPC reste inchangee. Le `Debug`
expose tailles et metadonnees de routage, sans bytes du payload opaque, valeurs
nonce/idempotence ou details d'erreur distante.

**Maturité :** contrat wire V1 stable à compatibilité stricte.
