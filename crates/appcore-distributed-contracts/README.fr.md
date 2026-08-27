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

V2 définit aussi un codec binaire sélectionné explicitement. Un magic fixe, la
version du codec, le type de message et la taille exacte encadrent un payload
Postcard borné; les octets de chunk restent natifs au lieu de base64. JSON ne
change pas et chaque frame ou reply binaire est limité à 256 Kio avant le
décodage. Un mismatch de codec est une erreur, jamais un fallback automatique.

Les rejets V2 utilisent `PeerRpcWireErrorV2` : code fixe, phase et
retryability autoritatifs, retry hint/corrélation bornés et message expurgé
contrôlé par le protocole. Un code inconnu devient l'unique résultat terminal
`unknown`. Le rejet string V1 figé possède un décodeur exact séparé et
n'utilise jamais de comparaison par sous-chaîne.

**Maturité :** V1 stable; contrat chunk V2 post-1.0 en développement.
