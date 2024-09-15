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

À utiliser pour réplication compatible, ordonnée et hash-chaînée. Ne pas
contourner identité/protocole ni l'interpréter comme RAFT, multi-master ou
résolution de conflits métier.

Le log fichier est limité à 256 MiB et l'outbox à 64 MiB. Les identifiants peer
et hashes de checkpoint sont validés à l'écriture et à la lecture. Le receiver
valide tout le batch, l'arithmétique de sequence et chaque limite de record
avant toute mutation du log ou checkpoint; un événement final invalide ne
laisse pas d'append partiel.

**Maturité :** profil RC conservateur stable avec décodage V1 strict.
