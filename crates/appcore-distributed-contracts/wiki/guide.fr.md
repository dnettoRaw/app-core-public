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

`peer_rpc::v2` est une famille indépendante et opt-in de frames. La frame open
fixe quota agrégé, taille/nombre de chunks et deadline; les chunks portent
séquence, encoding, taille décodée et digest exacts; commit lie taille et digest
totaux; cancel utilise une raison contrôlée. Les octets encodés utilisent une
chaîne JSON base64 canonique, pas un tableau d'entiers. V1 et V2 ont des modules et routes
séparés. Aucun parser ne détecte, met à niveau ou applique un fallback entre
eux.

**Maturité :** V1 stable; contrat chunk V2 post-1.0 en développement.
