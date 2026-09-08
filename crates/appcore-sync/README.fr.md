# appcore-sync

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
