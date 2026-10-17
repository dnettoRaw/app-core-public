# appcore-sync

Release candidate **`appcore-sync 1.0.2-rc.1`**. Sélectionner explicitement cette RC ; elle ne remplace pas les versions stables antérieures et ne certifie pas un déploiement. Les dépendances AppCore internes, lorsqu’elles existent, sont fixées sur cette série RC.


Tests locaux:

```bash
cargo test -p appcore-sync
```

**Responsabilité :** réplication leader-to-follower conservatrice et helpers de
durabilité locale.

**Dépendances internes :** `appcore-core`,
`appcore-distributed-contracts`, `appcore-ops`, `appcore-transport`.

**API principale :** node role/status/peer/heartbeat et `SyncMessage`; codec
wire V1; replication logs/snapshots; checkpoints et outbox mémoire/fichier;
receiver state/ack; follower client; transport HTTP; peer discovery; retry,
métriques et `SyncError`.
Les contrats content-envelope opaque sont réexportés pour les paquets sync
basés sur DNT sans exposer le plaintext au code de routage. La limite de
rétention `MAX_OPAQUE_MESSAGE_ID_BYTES` est aussi réexportée et vaut 1 024
octets.

`SyncConflict` est un contrat UI borné et sans payload contenant le pair, la
sequence, les SHA-256 local/distant et une raison typée.
`InMemorySyncConflictStore` enregistre les conflits et applique
`SyncConflictResolutionRequest` de manière idempotente ; rejouer la même clé
renvoie un receipt, tandis qu'une autre résolution ne peut pas remplacer la
première. Le store enregistre les décisions sans fusionner les payloads métier
ni contourner les règles leader-to-follower.

`HttpSyncTransport` possède un client HTTP réutilisable et borné.
`with_timeout_ms` conserve le délai V1 uniforme ; `with_timeouts` définit des
délais indépendants de connexion/admission, de lecture et d'écriture.

À utiliser pour réplication compatible, ordonnée et hash-chaînée. Ne pas
contourner identité/protocole ni l'interpréter comme RAFT, multi-master ou
résolution de conflits métier.

Le log fichier est limité à 256 MiB et l'outbox à 64 MiB. Les identifiants peer
et hashes de checkpoint sont validés à l'écriture et à la lecture. Le receiver
valide tout le batch, l'arithmétique de sequence et chaque limite de record
avant toute mutation du log ou checkpoint; un événement final invalide ne
laisse pas d'append partiel.

`FileSyncCheckpointStore` parcourt V1 avec un reader fixe de 16 Kio. La
validation au démarrage ne conserve aucune map de peers, et le lookup valide le
fichier complet en ne possédant que le hash demandé. Une mutation construit
encore une fois la map canonique triée, mais l'écrit directement avec un buffer
fixe sans conserver une autre `String` de la taille du fichier. Les plafonds
publics sont 8 Mio, 65 536 records non vides et 256 octets UTF-8 par ID de peer ;
chaque ligne est bornée avant d'agrandir le scratch. Les peers dupliqués gardent
la dernière valeur comme auparavant et la prochaine mutation réussie les
canonicalise.

`FileReplicationLog` parcourt V1 progressivement et ne conserve qu'un vecteur
trié compact de paires sequence/index de record, avec offsets, tailles et
digests par record, pas tous les payloads. L'implémentation en mémoire utilise
le même index plat trié et une recherche binaire, sans le surcoût des buckets
de hash pour les journaux locaux bornés. Les instances
concurrentes vérifient l'ancre de la chaîne de hash et ne lisent que le nouveau
tail ; un remplacement atomique par snapshot est traité comme une nouvelle
génération et reconstruit. `events_page` borne la lecture avant allocation à
1 024 enregistrements et 48 Mio. L'ancien `events_since` reste compatible au
niveau source mais refuse une lecture fichier au-delà de ces plafonds. Les outils
sync du déploiement doivent utiliser les pages ; aucune CLI sync Runtime n'est
distribuée. Le log reste borné à 256 Mio, chaque payload à 1 Mio et
l'index à 262 144 enregistrements. Les batches HTTP du Runtime s'arrêtent à
1 Mio d'événements bruts ; l'enveloppe JSON V1 encodée est bornée à 5 Mio afin
de transporter même la représentation numérique maximale d'un payload valide.
L'encodeur V1 sérialise directement l'identité, le message et les événements
empruntés dans la `String` de sortie requise. Il ne clone pas tout le batch avant
l'encodage, et son JSON reste identique octet par octet au contrat V1 owned.

`ReplicationSnapshot::try_from_records` consomme les paires sequence/payload
owned et déplace leurs allocations dans un snapshot V1 protégé par checksum.
`ReplicationSnapshot::validate` vérifie par référence les mêmes invariants de
format, nombre, payload, sequence et checksum, sans créer une seconde
collection de payloads. Un provider persistant peut donc valider avant mutation
avec un seul owner du snapshot sémantique. Les consommateurs mémoire qui
possèdent le snapshot peuvent appeler
`InMemoryReplicationLog::restore_snapshot_owned` pour valider puis déplacer les
payloads directement dans le log, sans copies simultanées.

Dans la version candidate `1.0.2-rc`, `FileSyncOutbox` utilise le journal binaire
append-only explicite `appcore-sync-outbox-v2`. Enqueue et ACK synchronisent une
seule frame chaînée par hash ; les lecteurs ne parcourent que le nouveau tail et
la compaction bornée conserve atomiquement les messages en attente. Seule une
frame finale incomplète est récupérable. Un fichier V1, sans version, futur ou
entièrement corrompu échoue de manière fermée. Videz V1 avant la mise à niveau
et V2 avant un rollback ; consultez
[`release/outbox-v2-migration.md`](../../release/outbox-v2-migration.md).

L'état résident de l'outbox fichier contient uniquement les identifiants de
batch, offsets du journal, tailles encodées, digests de payload et métadonnées
retry. Enqueue mesure le JSON pendant une passe bornée puis l'écrit avec un
buffer fixe de 64 Kio ; aucune copie encodée ne reste résidente. Les lectures de
tête et de page décodent et vérifient un seul message indexé à la fois. Chaque
ID est partagé avec l'état transactionnel du scan du tail ; refresh ne clone
que des handles au lieu de copier tous les identifiants en attente. Le
snapshot `messages()` conservé pour la compatibilité source retourne toujours
un `Vec` owned ; les consumers sensibles à la mémoire doivent utiliser les
pages bornées.

`InMemorySyncOutbox` obtient aussi la taille encodée exacte avec un compteur
JSON protégé contre l'overflow, sans allouer puis jeter un message encodé
complet. Un batch valide de 4 Mio a mesuré 10,92 ms p50 et 17,52 Mio de RSS de
pic sur Apple M1, contre 23,55 ms et 45,73 Mio avec le buffer temporaire.

La fenêtre des 10 000 batches traités du receiver conserve une allocation
partagée par `batch_id` entre la recherche de doublons et l'éviction selon
l'ordre d'acceptation. L'application de 10 000 batches avec des IDs de 128
octets a mesuré 58,27 ms p50 et 15,27 Mio de RSS de pic sur Apple M1, contre
61,29 ms et 17,45 Mio avec deux strings. Rejet duplicate et éviction du plus
ancien ne changent pas. Les frontières du receiver et de l'outbox rejettent les
IDs vides, les caractères de contrôle et les IDs dépassant 1 024 octets UTF-8
avant de les conserver. La fenêtre fixe est donc bornée en octets comme en
nombre d'éléments.

Le contrat additif de pagination `SyncOutbox` de la version candidate `1.0.2-rc`
expose `peek`, `stats`, `mark_attempt`, `next_ready` et des receipts partiels
ordonnés. Les lectures sont limitées à 1 024 messages et 48 Mio avant tout clone
de payload. Les providers mémoire et fichier implémentent une pagination et des
observations retry exactes. Les attempts et receipts ordonnés fichier sont des
frames hash-chaînées qui survivent au restart. Un provider peut utiliser
`encoded_sync_message_bytes` pour obtenir la taille JSON compacte exacte sans
copie encodée, puis `write_sync_message_json` pour transmettre cette même
représentation canonique à un writer borné. L'échappement reste compatible avec
Serde et les octets des événements utilisent un scratch buffer fixe de 16 Kio.
Le receipt est mesuré et sérialisé
directement par le writer fixe de 64 Kio ; le fixture maximal avec IDs échappés
ne matérialise plus son buffer JSON de 2 086 913 octets en production. Le scan
emprunte les IDs sans échappement à la frame. Un provider externe utilisant les
defaults de compatibilité compile encore : au plus le message de
tête, statistiques étendues inconnues et rejet explicite des attempts
persistées ou receipts multiples.

`FollowerSyncClient` utilise directement ce
contrat borné. Chaque échec de transport enregistre la readiness retry, le
succès applique un receipt exact et le drainage expose le dernier batch
acquitté pour faire progresser le checkpoint. Le snapshot complet
`pending_messages` reste disponible pour la compatibilité source ; les nouveaux
consommateurs doivent utiliser `pending_page` et `outbox_stats`.

Dans la version candidate `1.0.2-rc`, `ReplicationLog::len`, `last_index` et
`is_empty` retournent `SyncResult`. Les providers persistants exposent les
échecs d'observation au lieu de substituer zéro ou un état ancien. Les consumers
doivent traiter le résultat avant la mise à niveau ; voir
[`release/fallible-replication-log-observations.md`](../../release/fallible-replication-log-observations.md).

**Maturité :** profil RC conservateur stable avec décodage V1 strict.

Le défaut de `ReplicationLog::events_page` est un adaptateur de lecture complète
pour les providers externes : il valide les limites, appelle `events_since`,
puis déplace les payloads sélectionnés dans le résultat borné. Il ne borne pas
la lecture initiale. Les providers doivent redéfinir la pagination pour borner
nombre/octets avant lecture ou copie ; les providers internes mémoire/fichier
le font déjà. Une petite page ne prouve pas une matérialisation bornée. Test :
`cargo test -p appcore-sync --test external_log_paging`.

## Documentation stable

Identifiant stable : **ACR-012**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-012). Cet identifiant
permanent reste valable si la page du wiki est déplacée.

Pour les payloads opaques plus grands qu’une requête, `split_sync_payload`
crée des chunks bornés avec les empreintes SHA-256 du chunk et du payload
complet. `SyncChunkAssembler` accepte les chunks dans n’importe quel ordre,
expose les intervalles manquants sans payload et rend les rejouements
identiques idempotents. Les plages invalides, chevauchements et empreintes
divergentes sont refusés; le wire V1 reste inchangé.

[Migration d'enveloppes applicatives](../appcore-peer-rpc/wiki/transport-migration.fr.md).

## Causalité des révisions opaques

Cette section décrit des changements de source non publiés. Vérifiez le package exact du registre avant adoption par les consommateurs.

`ObjectRevisionV1`, `RevisionChainV1` et `decide_object_revision_v1` classent `already_applied`, `ancestor_echo`, `applicable`, `concurrent_branch` et `snapshot_required`. La chaîne accepte au plus 1 024 révisions et des identités de 128 octets. Authentifiez chaque événement et liez son tenant avant de construire les preuves : validation structurelle, non cryptographique. Liens absents, cycles, objet différent et réutilisation d’identité d’événement/révision sont refusés. Ni contenu identique ni dates ne prouvent l’ascendance. Une restauration volontaire utilise une nouvelle révision et un nouvel événement. Avancez séparément le checkpoint de transport lors d’un écho ; appelez `validate_checkpoint_head_v1` avant de modifier les métadonnées.

## Retransmission et sérialisation du checkpoint

Correction source non publiée : une répétition authentifiée identique reçoit un ACK accepté sans nouveau record ; la réutilisation divergente d’un `batch_id` conservé échoue avec `divergent batch_id reuse`. Le receiver conserve au plus 10 000 paires ID/digest. Cette fenêtre est locale au processus, pas une preuve permanente ; après redémarrage ou éviction, la politique existante des anciennes séquences s’applique. Authentifiez et isolez la source avant l’appel. Des receivers distincts doivent partager le handle du log lorsqu’ils partagent les checkpoints.

Lecture du checkpoint, vérification du chevauchement, append et publication du couple séquence/hash utilisent le même verrou du log. Les lots concurrents ne peuvent publier un ancien checkpoint inférieur ni mélanger séquence et hash de lots distincts. Une erreur provider peut encore laisser un log avancé et un checkpoint ancien ; la répétition vérifie le chevauchement avant publication. Le sender retire le reçu de l’outbox seulement après confirmation du transport. La perte du premier ACK entraîne une retransmission sûre puis le drainage. Les processus externes et handles indépendants nécessitent une coordination transactionnelle provider/deployment.

## Candidat de publication — 2026-10-14

`2.0.0-alpha.5`: modifications propres.

## Historique canonique par service (source non publiée)

`sync::canonical::FileCanonicalHistoryV1` conserve les métadonnées originales de `SyncMessage` et les octets des événements, avec la preuve transférable époque/writer de `CanonicalBatchV1`. `CanonicalStreamV1` identifie tenant/cluster/service indépendamment du Core ou de l’adresse courante. `CanonicalHeadV1` lie séquence, hash original du lot et un second digest du service, de l’époque, du writer et de l’historique. Les formats V1 existants du wire, receiver, log et snapshot restent inchangés ; aucune importation ou conversion automatique.

Réserver `create` au provisionnement initial ; le redémarrage utilise `open_existing`. Chaque opération recharge et valide l’état durable sous try-lock du système. L’écriture synchronise un temporaire exclusif, effectue un rename atomique puis synchronise le répertoire lorsque possible. Un ACK perdu accepte le même enregistrement persisté après redémarrage ; trous, IDs/chevauchements divergents, époques anciennes et changement de writer dans une époque sont refusés. Corruption ou stockage absent ne recréent jamais l’historique. Après échec de persistance, rouvrir et consulter le résultat : le rename peut avoir réussi.

`batches(offset, limit)` retourne jusqu’à 64 enregistrements complets ; authentifier tout l’enregistrement avant `replicate_authenticated`. Cela réplique l’historique, pas la matérialisation ni les transactions des effets applicatifs. Une époque historique ne vaut pas une concession courante. Le host authentifie tenant/service/source et autorité historique avant admission ; les hashes ne sont pas des signatures.

`with_verified_head(required, action)` compare la tête durable complète et conserve le verrou pendant le callback. La tête exigée provient d’une politique de durabilité fiable, jamais du candidat lui-même. Une réplique vide/retardée ou de même séquence avec un hash différent ne lance pas l’action. Dans le callback, composer un fence exact et courant autour du commit, toujours dans l’ordre historique-puis-autorité. La référence colocalisée est `FileControlPlane::with_fenced_service_write` ; ne pas réentrer dans les providers. Un `ServiceLeaseAuthority::check` distant est une observation, pas un commit distant atomique. La primitive seule ne promeut aucun nœud et ne délivre aucun certificat de disponibilité.

Découvrir le writer avec `ServiceLeaseAuthority::inspect` pour le même tenant/cluster/service, résoudre son Core par le discovery du déploiement et authentifier l’endpoint. Inspect peut retourner une concession expirée. L’ancien writer rejoint comme réplique ; toute divergence exige une réconciliation explicite par opérateur/snapshot validé par l’application. Ne pas réinitialiser la séquence ni changer la source pour masquer une divergence. Le rollback externe de l’autorité et de l’historique reste hors de cette référence ; la garde certifiée du stockage est nécessaire.

Limites : fichier encodé de 16 MiB, 1 024 lots, 16 384 événements et 4 MiB de payload brut total, chaque événement jusqu’à 1 MiB. Identités de 128 octets selon le profil ASCII alphanumérique/`-_.`. Cette petite référence recharge le fichier entier ; le décodage JSON alloue avant admission décodée, ce n’est pas un provider streaming pour grands logs. Exécuter les I/O dans des workers bloquants bornés. La capacité épuisée bloque les écritures sans éviction/compactage de preuve. Snapshots applicatifs, politique de confirmation des répliques et qualification physique du mesh restent séparés.

Chaque append réussi est immédiatement persisté ; une erreur du callback ne restaure pas un append précédent. Rouvrir et consulter la tête durable avant de répéter un résultat incertain.

## Durabilité des copies canoniques (source non publiée)

`DurabilityTargetV1` fixe le batch ID original, la tête complète, le Core writer et l’époque. `DurabilityPolicyV1` sélectionne explicitement `local`, `master` ou `master_and_replica`. `evaluate_durability_v1` retourne `DurabilityStatusV1` avec `confirmed`, `persisted_core_ids` et `missing_core_ids` ; l’attente ne réduit jamais automatiquement la politique. Master désigne le writer original de cette cible immuable, pas le Core actuellement découvert. Un retry conserve cible et politique ; attente/timeout n’annule pas l’écriture et n’autorise pas à réexécuter le handler applicatif.

| Politique | Copies canoniques exigées | Exposition restante à la perte |
| --- | --- | --- |
| `Local { core_id }` | Core local choisi | Perte de cette copie ; aucune confirmation master/réplique implicite |
| `Master` | Core writer original | Perte du stockage writer avant une autre copie confirmée |
| `MasterAndReplica { replica_core_id }` | Writer original et une réplique distincte choisie | Perte des deux copies ou domaine de panne partagé ; des IDs distincts ne prouvent pas des disques indépendants |

`FileCanonicalHistoryV1::durability_target` retrouve une limite exacte de lot conservé. `durability_receipt(target, local_core_id)` recharge/valide les octets sous verrou, vérifie tous les champs, synchronise fichier/répertoire lorsque supporté puis émet un reçu V1 déterministe. Lier `local_core_id` à l’identité du stockage du déploiement, jamais à un champ du demandeur. Une ancienne cible reste confirmable uniquement comme ancêtre exact conservé ; cela récupère une réponse perdue après redémarrage/nouveaux lots sans append supplémentaire. Aucun journal de reçus séparé ni changement du format historique. Genesis, milieu de lot, stockage absent/corrompu/occupé et cible divergente ne produisent pas de reçu.

Authentifier le reçu complet et lier `persisted_by_core_id` à l’émetteur authentifié avant évaluation. JSON et digests n’authentifient pas. `decode_durability_receipt_v1` limite l’entrée à 16 KiB et refuse versions absentes/inconnues avec `NO MORE SUPPORTED PLEASE UPDATE` ; au plus 16 reçus par évaluation. Une preuve répétée du même Core est idempotente et ne compte jamais deux fois. Chaque reçu doit correspondre à la cible complète ; un Core non sélectionné ne satisfait pas la politique. Le reçu n’est ni concession courante, ni probe de disponibilité, ni preuve de matérialisation applicative, ni certificat de consensus. La synchronisation dépend des garanties certifiées de la plateforme/filesystem.

Le déploiement persiste politique choisie, cible fiable et confirmations authentifiées dans sa frontière de récupération avant de déclarer le succès ou retirer le lot de l’outbox. Il doit sérialiser les mutations de file et lier le retrait au même contenu, sans course fondée uniquement sur l’ID. Ces primitives ne gardent aucune carte illimitée d’opérations et ne modifient pas automatiquement l’outbox. `FollowerSyncClient` conserve son contrat d’ACK de transport ; la durabilité répliquée exige la composition explicite de ce parcours. Le test conserve une véritable outbox fichier pendant l’attente, la redémarre après réponse perdue de la réplique et retire le lot uniquement après réception des deux reçus correspondants.

Ces politiques confirment les **copies d’historique canonique**, pas une transaction/outbox locale applicative avant attribution canonique. La durabilité offline local-first existante reste applicative. Une entrée locale d’outbox ne vaut pas confirmation master/réplique. Une cible confirmée fiable peut fournir la tête exigée à la promotion seulement si le déploiement la conserve et authentifie les preuves ; concession exacte courante et fence atomique restent obligatoires. Aucune intégration en production, qualification de coupure électrique ou garantie de perte zéro n’est affirmée.
