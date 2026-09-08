# appcore-sync-sqlite

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Persistance SQLite optionnelle post-1.0 pour l'état de synchronisation AppCore.

Le crate implémente les contrats existants de replication log, outbox et
checkpoint. Il fournit aussi des snapshots portables, des tombstones opaques
bornés, l'inspection d'intégrité et le backup/restore en ligne vérifié. Il
n'expose jamais la connexion SQLite et n'accepte aucun SQL, table, migration ou
workflow applicatif.

Chaque database utilise un schéma interne V2 transactionnel, WAL,
`synchronous=FULL`, un pool de connexions borné, un busy timeout, les limites
SQLite et une validation d'intégrité au startup. Les schémas inconnus, sans
version ou futurs échouent avec `NO MORE SUPPORTED PLEASE UPDATE`.

Le schéma V2 ajoute des compteurs attempt bornés et timestamps de readiness à
l'outbox. `peek` et `next_ready` sélectionnent les métadonnées nombre/octets
avant de lire les BLOBs; les stats ne chargent aucun payload et un receipt
partiel ne supprime qu'un préfixe ordonné exact dans une transaction. L'enqueue
mesure d'abord la taille JSON canonique exacte, puis écrit directement dans un
`zeroblob`; la comparaison des doublons, les lectures de pages et la validation
d'intégrité au startup transmettent également le contenu des BLOBs. Aucun
`Vec<u8>` de la taille du record encodé ne coexiste avec le message owned. Les
buffers du stream suivent la taille du record encodé jusqu'aux plafonds fixes
de 64 Kio en lecture et 1 Mio en écriture ; un petit record ne réserve jamais
ces maxima. Une database schema V1 connue migre atomiquement avec métadonnées
retry à zéro.
Conservez un backup antérieur pour rollback.

La création d'un snapshot portable déplace dans le snapshot V1 les payloads lus
depuis SQLite. Le restore valide par référence le snapshot owned par le caller,
compare son payload agrégé à `max_database_bytes` avant toute mutation et insère
directement ces records empruntés dans une transaction. Aucun replication log
complet en mémoire ni seconde collection de payloads ne coexiste avec le
snapshot.

Le descriptor déclare `transactions`, `locking`, `snapshot`, `online_backup` et
`multi_process`. Il ne déclare ni `streaming` ni `multi_host`.

Ce crate de développement n'est pas sélectionné par les manifests V1 stables et
n'est pas connecté au SDK. Les consumers directs font un opt-in
explicite. Voir
[`release/sqlite-sync-provider-v1.md`](../../release/sqlite-sync-provider-v1.md).

```bash
cargo test -p appcore-sync-sqlite
```

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

## Documentation stable

Identifiant stable : **ACR-026**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-026). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
