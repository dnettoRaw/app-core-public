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
partiel ne supprime qu'un préfixe ordonné exact dans une transaction. Une
database schema V1 connue migre atomiquement avec métadonnées retry à zéro.
Conservez un backup antérieur pour rollback.

Le descriptor déclare `transactions`, `locking`, `snapshot`, `online_backup` et
`multi_process`. Il ne déclare ni `streaming` ni `multi_host`.

Ce crate de développement n'est pas sélectionné par les manifests V1 gelés et
n'est pas connecté à `appcore-bin`. Les consumers directs font un opt-in
explicite. Voir
[`release/sqlite-sync-provider-v1.md`](../../release/sqlite-sync-provider-v1.md).

```bash
cargo test -p appcore-sync-sqlite
```
