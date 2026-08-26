# appcore-sync-sqlite

[English](guide.en.md) | [Português](guide.pt.md) |
[Basique](examples/basic.fr.md) | [Intermédiaire](examples/intermediate.fr.md)

**Couche :** integration. **Statut :** prerelease `0.1.0-alpha.2` publiée.

`SqliteSyncStore::open` résout le chemin vers un emplacement local stable,
rejette une cible database symlink, configure WAL et les limites SQLite,
exécute uniquement les migrations transactionnelles connues et vérifie
l'intégrité avant de retourner. Corruption complète et formats inconnus
échouent fermés avec des erreurs expurgées.

Un store crée des handles indépendants pour replication log, outbox,
checkpoints et tombstones opaques. Les clones partagent un pool d'au plus 32
connexions. L'admission writer et le busy wait ont une deadline. Reads,
snapshots, entries outbox, tombstones, pages et étapes de backup sont bornés.

Les snapshots portables utilisent `ReplicationSnapshot` V1. Le backup en ligne
utilise l'API SQLite et ne publie qu'un nouveau fichier vérifié. Le restore exige
aussi une nouvelle destination ; remplacer une database active n'est pas pris
en charge. Gardez database, `-wal` et `-shm` ensemble jusqu'à la fermeture de
tous les handles.

SQLite accepte des processus locaux indépendants sur un filesystem au locking
fiable. Les partages réseau et hosts concurrents sont hors profil. Le provider
ne contient aucun schéma applicatif et n'offre aucun accès SQL arbitraire.

Pour rollback, arrêtez l'admission, drainez/exportez l'outbox, créez un backup
vérifié et exportez un snapshot portable. La persistance fichier doit être créée
explicitement ; renommer la database n'est pas une migration.

## Certification

Le benchmark release sur source propre au commit `0f6f6d0` a réussi sous macOS
arm64 avec Rust 1.97.1. Pour 2 048 ajouts durables de 1 Kio et 2 048 lectures
ponctuelles, le p99 d'ajout était de 1,086 ms à 3 729 opérations/s et le p99 de
lecture de 0,583 ms à 6 578 opérations/s. La sauvegarde en ligne vérifiée de
3 182 592 octets a pris 73,870 ms ; le contrôle d'intégrité complet 15,675 ms.
La reproduction utilise `appcore-certification bottlenecks`, comme décrit dans
`release/sqlite-sync-provider-v1.md`.
