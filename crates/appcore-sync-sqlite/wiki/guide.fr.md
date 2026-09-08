# appcore-sync-sqlite

[English](guide.en.md) | [Português](guide.pt.md) |
[Basique](examples/basic.fr.md) | [Intermédiaire](examples/intermediate.fr.md)

**Couche :** integration. **Statut :** prerelease optionnelle. Version du workspace
`0.1.0-alpha.4`, revue le 2026-09-05 ; cette revue du code ne certifie ni la
publication au registre ni l'aptitude à la production.

`SqliteSyncStore::open` résout le chemin vers un emplacement local stable,
rejette une cible database symlink, configure WAL et les limites SQLite,
exécute uniquement les migrations transactionnelles connues et vérifie
l'intégrité avant de retourner. Corruption complète et formats inconnus
échouent fermés avec des erreurs expurgées.

Le schéma interne V2 fournit à `SqliteSyncOutbox` pagination bornée, stats sans
payload, métadonnées attempt/readiness durables et receipts partiels ordonnés
transactionnels. Les métadonnées de page sont sélectionnées avant la
matérialisation des BLOBs. Une database V1 connue migre atomiquement; rollback
exige le backup vérifié antérieur à la migration.

Un store crée des handles indépendants pour replication log, outbox,
checkpoints et tombstones opaques. Les clones partagent un pool d'au plus 32
connexions. L'admission writer et le busy wait ont une deadline. Reads,
snapshots, entries outbox, tombstones, pages et étapes de backup sont bornés.

L'admission outbox calcule la taille exacte du JSON canonique sans copie
encodée, puis transmet le record vers un BLOB SQLite incrémental. Le contrôle
des doublons, les lectures de pages et la validation au startup transmettent
aussi le BLOB ; le message owned ne partage donc jamais la mémoire avec un
second buffer encodé de la taille du record.
Le scratch de lecture et d'écriture suit la taille encodée et est plafonné à
64 Kio et 1 Mio respectivement ; les petits records ne réservent pas les
buffers maximaux.

Les snapshots portables utilisent `ReplicationSnapshot` V1. Le backup en ligne
utilise l'API SQLite et ne publie qu'un nouveau fichier vérifié. Le restore exige
aussi une nouvelle destination ; remplacer une database active n'est pas pris
en charge. Gardez database, `-wal` et `-shm` ensemble jusqu'à la fermeture de
tous les handles.

La création du snapshot transfère les allocations des payloads de la database
vers la valeur portable. Le restore portable valide cette valeur par référence,
rejette les octets agrégés au-delà de `max_database_bytes` avant de supprimer
une row, puis emprunte les records pendant son unique transaction de
remplacement. Sur Apple M1, le workload de restore de 32 Mio a mesuré 396,00 ms
p50 et 73,84 Mio de RSS de pic, contre 466,80 ms et 108,97 Mio avec deux
répliques temporaires des payloads.

SQLite accepte des processus locaux indépendants sur un filesystem au locking
fiable. Les partages réseau et hosts concurrents sont hors profil. Le provider
ne contient aucun schéma applicatif et n'offre aucun accès SQL arbitraire.

Pour rollback, arrêtez l'admission, drainez/exportez l'outbox, créez un backup
vérifié et exportez un snapshot portable. La persistance fichier doit être créée
explicitement ; renommer la database n'est pas une migration.

Chaque connexion du pool et connexion auxiliaire de backup/restore configure
et vérifie `cache_size=-2048` et `mmap_size=0`. Le cache est une cible suggérée
de 2 Mio, pas un plafond strict de heap. Huit connexions par défaut représentent
environ 16 Mio de cibles de cache, avant connexions auxiliaires, requêtes,
temporaires, WAL, payloads et surcoût de l'allocateur. Aucune politique globale
de heap SQLite n'est modifiée. Temporaires et croissance du WAL nécessitent
encore un budget de déploiement. Voir
[la sémantique du cache SQLite](https://www.sqlite.org/pragma.html#pragma_cache_size).

Les connexions demandent et vérifient aussi `temp_store=FILE` avant usage,
sans modifier le répertoire temporaire global. Cela ne garantit pas que tout
travail temporaire va sur disque : `SQLITE_TEMP_STORE=3` impose la mémoire
(notamment dans le build bundled Android). SQLite peut aussi garder des pages
temporaires en cache. Le déploiement doit vérifier son build et prévoir espace
temporaire privé, mémoire et nettoyage ; les builds mémoire exigent un budget
propre. Voir [les temporaires SQLite](https://www.sqlite.org/pragma.html#pragma_temp_store).

L'autocheckpoint WAL à 1 000 pages est un déclencheur, pas un plafond disque.
Un lecteur conservant une transaction peut empêcher le checkpoint complet
pendant que les writers agrandissent le WAL. Un test interne garde un lecteur
pendant huit appends de 1 Mio, observe plus de 1 000 frames, puis vérifie
troncature et intégrité après libération et réouverture. Il utilise du SQL
privé, sans ajouter d'API publique de checkpoint. Bornez lecteurs/backups et
surveillez WAL et espace disque ; ne supprimez jamais un WAL actif.

Les pages du replication log valident désormais la somme des `length(payload)`
avant de convertir un BLOB de la page en `Vec` Rust. Comptage, métadonnées et
payloads sont lus dans une même transaction deferred, empêchant un remplacement
concurrent de changer la page entre les passes. Une page sélectionnée trop
grande échoue toujours entièrement. Cette passe supplémentaire ne borne pas
le cache interne de SQLite et ne fournit pas de sortie streaming.

## Certification

Le benchmark release sur source propre au commit `0f6f6d0` a réussi sous macOS
arm64 avec Rust 1.97.1. Pour 2 048 ajouts durables de 1 Kio et 2 048 lectures
ponctuelles, le p99 d'ajout était de 1,086 ms à 3 729 opérations/s et le p99 de
lecture de 0,583 ms à 6 578 opérations/s. La sauvegarde en ligne vérifiée de
3 182 592 octets a pris 73,870 ms ; le contrôle d'intégrité complet 15,675 ms.
La reproduction utilise `appcore-certification bottlenecks`, comme décrit dans
`release/sqlite-sync-provider-v1.md`. Dans le workload actuel de 512 petites
entries, l'enqueue a demandé 255 676 octets du heap Rust sans rétention, avec
141 791 ns p99, sous les gates explicites de 2 Mio et 250 ms. Le scratch ajusté
a réduit les octets demandés par le workload SQLite complet de 578 081 344 à
8 251 670 (-98,57 %) et le delta de heap vivant de 1 083 528 à 233 600 octets
(-78,44 %).

Le runner du crate exerce aussi de grands chemins de données. Sur Apple M1,
macOS 27, trois processus mesurés et un warmup, l'enqueue brut de 16 Mio avait
un p50 de 249,76 ms, un pic RSS de 42,81 Mio et un delta RSS de workload de
19,62 Mio. La restauration de 32 enregistrements/32 Mio avait un p50 de
361,29 ms, un pic RSS de 73,95 Mio et un delta de workload de 2,55 Mio ; le
snapshot préparé existait déjà au checkpoint idle. Le rapport non versionné est
`target/appcore/bench/sync-sqlite-memory.json`.
