# appcore-sync-sqlite

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Optional post-1.0 SQLite persistence for AppCore synchronization state.

The crate implements the existing replication-log, outbox and checkpoint
contracts. It also provides portable snapshots, bounded opaque tombstones,
integrity inspection and verified online backup/restore. It never exposes a
SQLite connection or accepts application SQL, tables, migrations or workflows.

Every database uses transactional internal schema V2, WAL, `synchronous=FULL`,
a bounded connection pool, busy timeout, SQLite runtime limits and startup
integrity validation. Unknown, unversioned and future schemas fail with
`NO MORE SUPPORTED PLEASE UPDATE`.

Schema V2 adds bounded attempt counters and readiness timestamps to the outbox.
`peek` and `next_ready` select count/byte metadata before reading BLOBs; stats
contain no payload and a partial receipt deletes only an exact ordered prefix
in one transaction. A known schema V1 database migrates atomically with zeroed
retry metadata. Preserve a pre-migration backup for rollback.

The capability descriptor declares `transactions`, `locking`, `snapshot`,
`online_backup` and `multi_process`. It deliberately does not declare
`streaming` or `multi_host`.

This development crate is not selected by frozen V1 manifests and is not wired
into `appcore-bin`. Direct consumers opt in explicitly. See
[`release/sqlite-sync-provider-v1.md`](../../release/sqlite-sync-provider-v1.md).

```bash
cargo test -p appcore-sync-sqlite
```
