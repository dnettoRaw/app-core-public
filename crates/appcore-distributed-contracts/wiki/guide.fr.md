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

`OpaqueEnvelopeDeduplicator` conserve une allocation partagée pour chaque ID de
message accepté entre le set d'appartenance et la file d'ordre d'acceptation.
L'API publique, le résultat duplicate et l'éviction FIFO bornée ne changent
pas. Avec 65 536 IDs distincts de 128 octets sur Apple M1, le p50 est passé de
37,55 ms à 32,83 ms et le RSS de pic de 35,86 Mio à 27,25 Mio.
`validate_transport` et `accept` rejettent les IDs vides, les caractères de
contrôle et les IDs dépassant `MAX_OPAQUE_MESSAGE_ID_BYTES` (1 024 octets
UTF-8). Le rejet précède toute allocation ou éviction FIFO.

`peer_rpc::v2` est une famille indépendante et opt-in de frames. La frame open
fixe quota agrégé, taille/nombre de chunks et deadline; les chunks portent
séquence, encoding, taille décodée et digest exacts; commit lie taille et digest
totaux; cancel utilise une raison contrôlée. Les octets encodés utilisent une
chaîne JSON base64 canonique, pas un tableau d'entiers. V1 et V2 ont des modules
et routes séparés. L'encodage lisible émet le base64 avec des buffers scratch
fixes de 3 Kio en entrée et 4 Kio en sortie. Le décodage emprunte la chaîne JSON
encodée lorsque le désérialiseur le permet, puis alloue uniquement les octets
décodés nécessaires. Les fixtures exactes ne changent pas. Aucun parser ne
détecte, met à niveau ou applique un fallback entre versions.

Le codec binaire V2 optionnel est une représentation séparée et sélectionnée
explicitement. Le marqueur fixe `APCRPC2B`, la version du codec, le type
frame/reply et la taille exacte lient un payload Postcard borné. Les
sérialiseurs non humains transportent les chunks comme octets natifs; la
représentation JSON existante reste en base64 canonique. Encodage et décodage
reçoivent la limite de l'appelant et appliquent toujours le plafond protocole
de 256 Kio. Un mismatch de type, marqueur, version, taille ou codec échoue avant
d'atteindre une implémentation.

`PeerRpcWireErrorV2` transporte des métadonnées fixes `code`, `phase` et
`retryable`, des `retry_after_ms` et `correlation_id` optionnels et bornés, et
un message expurgé exact. Le décodage valide toute la matrice. Des métadonnées
connues contradictoires sont invalides; un code inconnu abandonne message/hint
et devient `unknown` terminal. Séparément, `PeerRpcRemoteErrorV1` ne décode que
les strings V1 stables exactes : le texte distant libre ne choisit jamais le
retry.

**Maturité :** V1 stable; contrat chunk V2 post-1.0 en développement.
