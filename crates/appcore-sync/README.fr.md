# appcore-sync

**Responsabilité :** réplication leader-to-follower conservatrice et helpers de
durabilité locale.

**Dépendances internes :** `appcore-core`,
`appcore-distributed-contracts`, `appcore-ops`, `appcore-transport`.

**API principale :** node role/status/peer/heartbeat et `SyncMessage`; codec
wire V1; replication logs/snapshots; checkpoints et outbox mémoire/fichier;
receiver state/ack; follower client; transport HTTP; peer discovery; retry,
métriques et `SyncError`.
Les contrats content-envelope opaque sont réexportés pour les paquets sync
basés sur DNT sans exposer le plaintext au code de routage.

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

Dans la version candidate `1.0.2-rc`, `FileSyncOutbox` utilise le journal binaire
append-only explicite `appcore-sync-outbox-v2`. Enqueue et ACK synchronisent une
seule frame chaînée par hash ; les lecteurs ne parcourent que le nouveau tail et
la compaction bornée conserve atomiquement les messages en attente. Seule une
frame finale incomplète est récupérable. Un fichier V1, sans version, futur ou
entièrement corrompu échoue de manière fermée. Videz V1 avant la mise à niveau
et V2 avant un rollback ; consultez
[`release/outbox-v2-migration.md`](../../release/outbox-v2-migration.md).

Le contrat additif de pagination `SyncOutbox` de la version candidate `1.0.2-rc`
expose `peek`, `stats`, `mark_attempt`, `next_ready` et des receipts partiels
ordonnés. Les lectures sont limitées à 1 024 messages et 48 Mio avant tout clone
de payload. Les providers mémoire et fichier implémentent une pagination et des
observations retry exactes. Les attempts et receipts ordonnés fichier sont des
frames hash-chaînées qui survivent au restart. Un provider externe antérieur à
1.5 compile encore avec des defaults conservateurs : au plus le message de
tête, statistiques étendues inconnues et rejet explicite des attempts
persistées ou receipts multiples.

`FollowerSyncClient` et la CLI sync du Runtime utilisent directement ce
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
