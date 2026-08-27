# appcore-sync

Le contrat d'observation de la version candidate 1.5 est faillible :
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
basés sur DNT sans exposer le plaintext au code de routage.

`HttpSyncTransport` possède un client HTTP réutilisable et borné. Utilisez
`with_timeout_ms` pour le délai V1 uniforme ou `with_timeouts` pour des délais
indépendants de connexion/admission, de lecture et d'écriture.

À utiliser pour réplication compatible, ordonnée et hash-chaînée. Ne pas
contourner identité/protocole ni l'interpréter comme RAFT, multi-master ou
résolution de conflits métier.

Le log fichier est limité à 256 MiB et l'outbox à 64 MiB. Les identifiants peer
et hashes de checkpoint sont validés à l'écriture et à la lecture. Le receiver
valide tout le batch, l'arithmétique de sequence et chaque limite de record
avant toute mutation du log ou checkpoint; un événement final invalide ne
laisse pas d'append partiel.

L'outbox fichier de la version candidate 1.5 est le journal binaire
append-only V2 explicite. Enqueue et ACK ajoutent et synchronisent une frame
ordinale chaînée par hash ; les instances actives ne parcourent que le nouveau
tail. La compaction atomique change la génération et conserve les records en
attente. Le startup tronque uniquement une frame finale incomplète et échoue de
manière fermée en cas de corruption complète, duplication, réordonnancement ou
version incompatible. V1 n'est jamais déduit ni converti : videz V1 avant la
mise à niveau et V2 avant le rollback selon le
[runbook de migration](../../../release/outbox-v2-migration.md).

L'extension outbox de la version candidate 1.5 pagine avec
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
tandis qu'une frame finale incomplète conserve le préfixe non acquitté.

Le follower pilote directement `next_ready`, `mark_attempt` et les receipts
exacts. Utilisez `pending_page`, `outbox_stats` et
`flush_pending_with_progress` pour l'inspection bornée et la progression du
checkpoint. La livraison Runtime n'appelle jamais le snapshot complet de
compatibilité.

**Maturité :** profil RC conservateur stable avec décodage V1 strict.
