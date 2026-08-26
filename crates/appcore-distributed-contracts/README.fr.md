# appcore-distributed-contracts

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

Peer RPC V2 est une famille séparée et opt-in de frames dans `peer_rpc::v2`.
Les frames open, chunk, commit et cancel déclarent exactement protocole,
identité, séquence, tailles décodées, deadline et intégrité. Les octets encodés
utilisent une chaîne JSON base64 canonique, jamais un tableau d'entiers. V1 reste uniquement
dans `peer_rpc::v1`; aucune implémentation ne doit inférer ou convertir les
versions.

**Maturité :** V1 stable; contrat chunk V2 post-1.0 en développement.
