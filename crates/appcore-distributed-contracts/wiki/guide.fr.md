# appcore-distributed-contracts

Release candidate **`appcore-distributed-contracts 1.0.2-rc.1`**. Sélectionner explicitement cette RC ; elle ne remplace pas les versions stables antérieures et ne certifie pas un déploiement. Les dépendances AppCore internes, lorsqu’elles existent, sont fixées sur cette série RC.


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

## Projection de diagnostic opérationnel

Cette section décrit des changements de source non publiés. Vérifiez le package exact du registre avant adoption par les consommateurs.

`peer_rpc::v2::PeerRpcDiagnosticV2::from_wire` valide la matrice d’erreurs existante avant de conserver code, phase, retry et corrélation avec un `CoreId` fiable. `PeerRpcDiagnosticActionV2` fournit une suggestion contrôlée ; les codes inconnus sont expurgés et sans retry. Authentifiez et filtrez l’export par tenant ; ne rejouez jamais automatiquement un command ambigu. Le schéma wire existant reste inchangé ; ce contrat ne dépend d’aucune implémentation.

## Candidat de publication — 2026-10-14

`2.0.0-alpha.4`: modifications propres.


## Autorité exacte par service — source non publiée

`ServiceLeaseAuthority` est un contrat additif dans `appcore-distributed-contracts::control_plane::lease_authority`. Acquisition, renouvellement exact, libération exacte, inspection et check utilisent l’horloge serveur. `FileControlPlane` impose TTL 1–30000 ms : une acquisition du même Core retourne le grant vivant sans prolongation ; renew ne réacquiert pas une lease expirée et refuse les grants modifiés. Écriture des données et coordination des updates utilisent des `ServiceId` distincts choisis par le déploiement. Heartbeat, rôle et epochs HA Gateway ne confèrent aucune autorité sur les données.

Au restart utiliser `FileControlPlane::open_existing_with_clock` : état absent/corrompu refusé, aucun nouveau domaine d’epochs créé. Le provisionnement initial via `with_clock` est explicite. Les opérations utilisent un verrou OS sans attente et signalent contention/indisponibilité. `with_fenced_service_write` vérifie le grant exact et exécute un commit local fiable et borné sous le même verrou ; un takeover ne peut chevaucher ce commit même si la lease expire pendant celui-ci. Le callback ne doit pas réentrer dans l’arbitre ; le host possède bornes I/O et idempotence applicative. Un check distant reste une observation : le storage distant doit appliquer le fencing atomiquement au commit. Aucun check mémorisé n’autorise une écriture offline.

Gateway ajoute la composition explicite `make_service_lease_router(gateway, authority, replay)`, absente du router default. POST `/v1/gateway/service-lease` accepte `ServiceLeaseRequestV1` (version=1, identity, service_id, action) ; actions acquire/renew/release/inspect/check. Authentification obligatoire même en insecure-testing. Le domaine isolé `service_lease_token_claims` exige nom command/query exact, subject Core, hash complet du request et JTI unique jusqu’à 96 octets. La durée du credential ne dépasse ni le TTL demandé à l’acquisition ni l’expiration du grant pour les actions liées. Tokens relay, scope/action/epoch modifiés et wildcards sont refusés. L’émetteur autorise tenant/cluster/Core/service/action avant signature ; les clients ne reçoivent pas le secret de signature.

Composer un `PeerNonceStore` durable dédié partagé entre restarts/répliques de l’arbitre, par exemple `FilePeerNonceStore` sur stockage certifié. Aucun default implicite. Requests limités à 16 KiB et huit opérations sans file d’admission. Authentification/replay I/O et appels provider s’exécutent sur workers bloquants hors du polling async. Le déploiement possède deadlines I/O, drainage, TLS, horloge partagée et conservation des états. Réponses : 401 sans credential, 403 invalide/rejoué, 409 grant obsolète/conflit, 503 autorité indisponible/en contention ou saturation. Aucun succès offline. Échec provider ou réponse perdue reste ambigu : inspecter avec un nouveau credential sans deviner le succès ni rejouer une écriture avec un nouvel epoch.

Limites : Core est l’identité d’écriture, pas le PID ; partager ses credentials partage son autorité. Provisionner des Cores distincts pour les writers concurrents et un service logique stable pour la découverte. Référence sans consensus, sans arbitre HA à disques indépendants et sans fencing de storage distant. Perte/rollback total de l’état ou du replay nécessite récupération contrôlée/nouveau domaine ; supprimer les fichiers ou utiliser le constructeur initialisant n’est pas une récupération. Wire/storage V1 et APIs existantes préservés. Intégration consommateur, publication registry et qualification DNET/partition physique restent à faire.
