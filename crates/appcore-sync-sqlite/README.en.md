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
in one transaction. Enqueue measures the exact canonical JSON length first,
then writes directly to a `zeroblob`; duplicate comparison, page reads and
startup integrity validation also stream BLOB content. No encoded
record-sized `Vec<u8>` coexists with the owned message. The stream buffers use
the encoded record size up to fixed 64 KiB read and 1 MiB write ceilings, so a
small record never reserves either maximum. A known schema V1 database migrates
atomically with zeroed
retry metadata. Preserve a pre-migration backup for rollback.

Portable snapshot creation moves the payloads read from SQLite into the V1
snapshot. Restore validates the caller-owned snapshot by reference, checks its
aggregate payload against `max_database_bytes` before mutation and inserts
directly from those borrowed records in one transaction. No complete in-memory
replication log or second payload collection coexists with the snapshot.

The capability descriptor declares `transactions`, `locking`, `snapshot`,
`online_backup` and `multi_process`. It deliberately does not declare
`streaming` or `multi_host`.

This development crate is not selected by stable V1 manifests or by the SDK.
Direct consumers opt in explicitly. See
[`release/sqlite-sync-provider-v1.md`](../../release/sqlite-sync-provider-v1.md).

```bash
cargo test -p appcore-sync-sqlite
```

Each pool connection and backup/restore auxiliary connection explicitly sets
and checks `cache_size=-2048` and `mmap_size=0`. The cache setting is a suggested
2 MiB target, not a hard heap cap. Eight default pool connections imply about
16 MiB of cache targets before auxiliary connections, queries, temporary data,
WAL, payloads and allocator overhead. No process-global SQLite heap policy is
changed. Temporary storage and WAL growth still require deployment budgets.
See [SQLite cache semantics](https://www.sqlite.org/pragma.html#pragma_cache_size).

Connections also request and verify `temp_store=FILE` before use, without
changing the process-global temporary directory. This is not a guarantee that
all temporary work goes to disk: `SQLITE_TEMP_STORE=3` overrides the request
(the bundled Android build uses it). SQLite may also retain temporary pages
in cache. Deployments must inspect their build and budget private temporary
storage, memory and cleanup; memory-only builds need a separate memory budget.
See [SQLite temporary storage](https://www.sqlite.org/pragma.html#pragma_temp_store).

WAL autocheckpoint at 1,000 pages is a trigger, not a disk-space cap. A held
reader can prevent complete checkpointing while writers keep extending the
WAL. An internal regression holds one read transaction across eight 1 MiB
appends, observes more than 1,000 frames, then verifies truncation and intact
records after releasing the reader and reopening. The test uses private SQL;
it does not add a public checkpoint API. Bound readers/backups and monitor WAL
and filesystem capacity in deployment; never delete a live WAL to reclaim space.

Replication-log pages now validate the aggregate `length(payload)` values
before converting any page BLOB into a Rust `Vec`. Count, metadata and payload
reads share one deferred transaction, so concurrent replacement cannot change
the admitted page between passes. An oversized selected page still fails as
a whole, preserving the existing contract. This adds a metadata pass and does
not bound SQLite's internal cache or provide streaming output.

## Stable documentation

Stable ID: **ACR-026**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-026). This permanent ID
remains valid if the wiki page moves.
