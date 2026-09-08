# appcore-distributed-contracts

Tests locaux:

```bash
cargo test -p appcore-distributed-contracts
```

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

`OpaqueEnvelopeDeduplicator` conserve une seule allocation partagée par ID de
message accepté entre ses index d'appartenance et d'ordre d'acceptation.
L'éviction FIFO bornée et les décisions de duplication ne changent pas. La
rétention de 65 536 IDs distincts de 128 octets a mesuré 32,83 ms p50 et
27,25 Mio de RSS de pic sur Apple M1, contre 37,55 ms et 35,86 Mio avec deux
strings. La validation transport et la déduplication rejettent les IDs vides,
les caractères de contrôle et les IDs dépassant
`MAX_OPAQUE_MESSAGE_ID_BYTES` (1 024 octets UTF-8) avant rétention.

Peer RPC V2 est une famille séparée et opt-in de frames dans `peer_rpc::v2`.
Les frames open, chunk, commit et cancel déclarent exactement protocole,
identité, séquence, tailles décodées, deadline et intégrité. Les octets encodés
utilisent une chaîne JSON base64 canonique, jamais un tableau d'entiers.
L'encodage lisible émet cette chaîne avec des buffers scratch fixes de 3 Kio en
entrée et 4 Kio en sortie; le décodage emprunte la chaîne JSON encodée lorsque
le désérialiseur le permet. Le wire exact ne change pas. V1 reste uniquement
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
`unknown`. Le rejet string V1 stable possède un décodeur exact séparé et
n'utilise jamais de comparaison par sous-chaîne.

**Maturité :** V1 stable; contrat chunk V2 post-1.0 en développement.

## Documentation stable

Identifiant stable : **ACR-006**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-006). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
