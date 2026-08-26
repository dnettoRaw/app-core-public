# appcore-sync-sqlite

[Português](guide.pt.md) | [Français](guide.fr.md) |
[Basic](examples/basic.en.md) | [Intermediate](examples/intermediate.en.md)

**Layer:** integration. **Status:** `0.1.0-alpha.2` published prerelease.

`SqliteSyncStore::open` resolves the database path to a stable local location,
rejects a symlink database target, configures WAL and bounded SQLite limits,
runs only known transactional migrations and checks integrity before returning.
Complete corruption and unknown formats fail closed with redacted errors.

One store creates independent handles for:

- `SqliteReplicationLog`;
- `SqliteSyncOutbox`;
- `SqliteSyncCheckpointStore`;
- `SqliteSyncTombstoneStore`.

Clones share a pool admitting at most 32 connections. Writer admission and
SQLite busy waits have deadlines. Log reads, snapshots, outbox entries,
tombstones, database pages and backup steps have explicit limits.

Portable snapshots use `ReplicationSnapshot` V1. Online backup uses SQLite's
backup API and publishes only a verified new file. Restore also targets a new
path; replacing a live database is intentionally unsupported. Keep a database,
its `-wal` file and its `-shm` file together until all handles close.

SQLite supports independent local processes on a filesystem with reliable
locking. Network shares and concurrent hosts are outside this profile. The
provider contains no application schema and offers no arbitrary SQL escape.

For rollback, stop admission, drain/export the outbox, create a verified backup
and export a portable replication snapshot. File persistence must be created
explicitly from public artifacts; database renaming is not a migration.

## Certification

The clean release benchmark at `0f6f6d0` passed on macOS arm64 with Rust
1.97.1. For 2,048 durable 1 KiB appends and 2,048 point reads, append p99 was
1.086 ms at 3,729 operations/s and read p99 was 0.583 ms at 6,578 operations/s.
A verified 3,182,592-byte online backup took 73.870 ms; the full integrity scan
took 15.675 ms. Reproduce it with `appcore-certification bottlenecks` as
documented in `release/sqlite-sync-provider-v1.md`.
