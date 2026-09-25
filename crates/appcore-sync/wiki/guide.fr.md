# appcore-sync

Le contrat d'observation de la version candidate `1.0.2-rc` est faillible :
`ReplicationLog::len`, `last_index` et `is_empty` retournent `SyncResult`.
Traitez l'erreur comme une santé de persistance inconnue ; ne la remplacez
jamais par zéro ou une valeur en cache. Migration et rollback sont décrits dans
[`release/fallible-replication-log-observations.md`](../../../release/fallible-replication-log-observations.md).

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** réplication leader-to-follower conservatrice et helpers de
durabilité locale.

**Dépendances internes :** `appcore-core`,
`appcore-distributed-contracts`, `appcore-ops`, `appcore-transport`.

**API principale :** node role/status/peer/heartbeat et `SyncMessage`; codec
wire V1; replication logs/snapshots; checkpoints et outbox mémoire/fichier;
receiver state/ack; follower client; transport HTTP; peer discovery; retry,
métriques et `SyncError`.
Les contrats content-envelope opaque sont réexportés pour les paquets sync
basés sur DNT sans exposer le plaintext au code de routage. Leur limite publique
de rétention `MAX_OPAQUE_MESSAGE_ID_BYTES` est de 1 024 octets UTF-8.

`HttpSyncTransport` possède un client HTTP réutilisable et borné. Utilisez
`with_timeout_ms` pour le délai V1 uniforme ou `with_timeouts` pour des délais
indépendants de connexion/admission, de lecture et d'écriture.

À utiliser pour réplication compatible, ordonnée et hash-chaînée. Ne pas
contourner identité/protocole ni l'interpréter comme RAFT, multi-master ou
résolution de conflits métier.

Pour les conflits prêts pour l'UI, utilisez `SyncConflict` et
`InMemorySyncConflictStore`. Les enregistrements contiennent seulement des
métadonnées bornées de pair/sequence, SHA-256 et raison typée. Les demandes de
résolution sont idempotentes et une autre décision ne remplace pas l'existante ;
la fusion des payloads métier reste hors de ce crate.

Le log fichier est limité à 256 MiB et l'outbox à 64 MiB. Les identifiants peer
et hashes de checkpoint sont validés à l'écriture et à la lecture. Le receiver
valide tout le batch, l'arithmétique de sequence et chaque limite de record
avant toute mutation du log ou checkpoint; un événement final invalide ne
laisse pas d'append partiel.

Le fichier checkpoint V1 accepte au plus 8 Mio et 65 536 records non vides,
avec des IDs de peer limités à 256 octets UTF-8. `FileSyncCheckpointStore`
valide une ligne bornée à la fois via un reader fixe de 16 Kio. Le démarrage ne
conserve aucune map décodée ; un lookup parcourt et valide tout le fichier mais
n'alloue que si sa cible existe. Une mutation construit une seule fois la map
canonique triée puis la transmet au fichier temporaire atomique : aucune
`String` complète d'entrée ou de sortie ne coexiste avec cette map. Les entrées
dupliquées conservent la dernière valeur du comportement V1 et la prochaine
mutation n'écrit que l'entrée canonique.

`FileReplicationLog` parcourt une ligne V1 bornée à la fois et ne conserve qu'un
index compact trié de sequence vers record, avec offsets, tailles et digests.
Le journal en mémoire utilise le même index plat trié et une recherche binaire,
sans conserver de buckets de hash pour les journaux locaux bornés. Les payloads
sont décodés à la demande, un par un. Un append verrouillé vérifie la dernière ancre de la chaîne de hash
et ne parcourt que les octets ajoutés par une autre instance ; un remplacement
atomique par snapshot invalide l'ancre et reconstruit progressivement l'index.
Utilisez `events_page(index, max_records, max_bytes)` avec des plafonds de 1 024
enregistrements et 48 Mio. La méthode complète de compatibilité refuse les
lectures fichier plus grandes. Les outils du déploiement doivent utiliser les
pages ; aucune CLI sync Runtime n'est distribuée. Les plafonds sont
256 Mio par fichier, 1 Mio par payload et 262 144 enregistrements. Les pages
HTTP du Runtime transportent au plus 1 Mio d'événements bruts dans une enveloppe
V1 encodée bornée à 5 Mio, y compris la pire expansion du tableau d'octets JSON.
L'encodeur wire emprunte l'identité, le message et chaque événement pendant
l'écriture de cette `String` de sortie requise. Il évite ainsi un second batch
complet en mémoire tout en préservant le JSON V1 owned exact et la validation du
node source.

Créez les snapshots portables à partir de payloads déjà owned avec
`ReplicationSnapshot::try_from_records` ; chaque `Vec<u8>` est transféré dans
le snapshot. Les providers appellent `ReplicationSnapshot::validate` via
`&self` pour vérifier version, nombre, taille par record, sequences non-nulles
uniques et checksum sans cloner la collection de payloads. La validation doit
se terminer avant qu'une transaction de restore modifie l'état durable. Un
consommateur mémoire qui possède le snapshot peut utiliser
`InMemoryReplicationLog::restore_snapshot_owned` pour valider puis déplacer les
payloads dans le log sans conserver les deux collections.

L'outbox fichier de la version candidate `1.0.2-rc` est le journal binaire
append-only V2 explicite. Enqueue et ACK ajoutent et synchronisent une frame
ordinale chaînée par hash ; les instances actives ne parcourent que le nouveau
tail. La compaction atomique change la génération et conserve les records en
attente. Le startup tronque uniquement une frame finale incomplète et échoue de
manière fermée en cas de corruption complète, duplication, réordonnancement ou
version incompatible. V1 n'est jamais déduit ni converti : videz V1 avant la
mise à niveau et V2 avant le rollback selon le
[runbook de migration](../../../release/outbox-v2-migration.md).

L'index V2 en mémoire conserve uniquement l'identifiant de batch, l'ordinal,
l'offset des données, la taille encodée, le digest du payload et les métadonnées
retry. Enqueue mesure et hash d'abord le JSON, puis sérialise le même message
directement avec un buffer fixe de 64 Kio. `front`, `peek` et `next_ready`
cherchent le record indexé, décodent un seul message et vérifient sa taille et
son digest exacts. Le journal possède ainsi le payload sans devenir la source
de l'ordre de livraison ou de l'état retry. L'ID de batch est un `Arc<str>`
partagé entre l'index actif et le scan transactionnel du tail. Refresh ne clone
donc que des handles, pas jusqu'à 1 024 octets d'identifiant pour chaque message
en attente ; l'état enqueue lu partage la même allocation avec son opération
pending. Utilisez les méthodes paginées avec
un budget mémoire strict : `messages()` doit matérialiser tous les messages
demandés car son résultat public est un `Vec`.

Le provider mémoire mesure cette même taille JSON exacte analytiquement avec une
arithmétique protégée contre l'overflow. `encoded_sync_message_bytes` fournit ce
compte aux providers d'intégration, tandis que `write_sync_message_json`
transmet la représentation identique et compatible avec Serde via un scratch
buffer fixe de 16 Kio pour les événements. Aucun chemin ne crée un second
message encodé complet uniquement pour décider l'admission, les limites de
page, la persistance ou `pending_bytes`.
Pour un batch valide de 4 Mio sur Apple M1, le p50 est passé de 23,55 ms à
10,92 ms et le RSS de pic de 45,73 Mio à 17,52 Mio.

Le receiver conserve également une allocation partagée par `batch_id` traité
entre son set de doublons et sa file ordonnée d'éviction. La fenêtre reste fixée
à 10 000 IDs. L'application de 10 000 batches avec des IDs de 128 octets sur
Apple M1 a mesuré 58,27 ms p50 et réduit le RSS de pic de 17,45 Mio à 15,27 Mio,
sans modifier le rejet duplicate ni l'éviction du plus ancien. Les frontières
du receiver et de l'outbox rejettent un ID vide, les caractères de contrôle ou
plus de 1 024 octets UTF-8 avant de conserver le message. `SyncMessage::new`
reste un constructeur de données infaillible ; l'acceptation est décidée à ces
frontières avec état.

L'extension outbox de la version candidate `1.0.2-rc` pagine avec
`peek(limit, max_bytes)`, expose des `stats` sans payload, enregistre la
readiness retry avec `mark_attempt`, sélectionne uniquement le préfixe ordonné
prêt avec `next_ready` et applique des receipts partiels de préfixe exact. Les
plafonds globaux sont 1 024 messages et 48 Mio. Les defaults de compatibilité
n'appellent jamais `messages()` : les providers antérieurs à l'extension
exposent un seul message de tête immédiat, des statistiques étendues inconnues
et des erreurs explicites pour l'état qu'ils ne peuvent pas persister.

`FileSyncOutbox` enregistre chaque attempt du message de tête et chaque receipt
validé comme frame V2 bornée et hash-chaînée. Les compteurs/readiness retry
survivent au restart ; une attempt ou un receipt complet corrompu échoue fermé,
tandis qu'une frame finale incomplète conserve le préfixe non acquitté. Le JSON
du receipt est d'abord mesuré puis sérialisé directement avec le writer fixe de
64 Kio. Le fixture maximal de 1 024 IDs échappés contient 2 086 913 octets et
n'est plus conservé dans un `Vec` de production supplémentaire. Le scan
emprunte les IDs sans échappement à la frame existante et n'alloue les chaînes
d'identifiant que si un déséchappement est nécessaire.

Le follower pilote directement `next_ready`, `mark_attempt` et les receipts
exacts. Utilisez `pending_page`, `outbox_stats` et
`flush_pending_with_progress` pour l'inspection bornée et la progression du
checkpoint. La livraison Runtime n'appelle jamais le snapshot complet de
compatibilité.

Le défaut de `ReplicationLog::events_page` est un adaptateur de lecture complète
pour les providers externes : il valide les limites, appelle `events_since`,
puis déplace les payloads sélectionnés dans le résultat borné. Il ne borne pas
la lecture initiale. Les providers doivent redéfinir la pagination pour borner
nombre/octets avant lecture ou copie ; les providers internes mémoire/fichier
le font déjà. Une petite page ne prouve pas une matérialisation bornée. Test :
`cargo test -p appcore-sync --test external_log_paging`.

**Maturité :** profil RC conservateur stable avec décodage V1 strict.

Pour un transfert opaque reprenable, utilisez `split_sync_payload` puis
alimentez `SyncChunkAssembler` avec les chunks dans n’importe quel ordre.
Reprenez depuis les intervalles sans payload de `progress().missing` et
appelez `assemble` lorsque tous les octets sont présents. L’adaptateur borne
les tailles, vérifie les deux SHA-256, accepte les doublons identiques et
refuse les chevauchements divergents sans modifier V1.
