# appcore-sync-sqlite

[Português](guide.pt.md) | [Français](guide.fr.md) |
[Basic](examples/basic.en.md) | [Intermediate](examples/intermediate.en.md)

**Layer:** integration. **Status:** optional prerelease. Workspace version
`0.1.0-alpha.4`, reviewed on 2026-09-05; this source review does not attest
registry publication or production certification.

`SqliteSyncStore::open` resolves the database path to a stable local location,
rejects a symlink database target, configures WAL and bounded SQLite limits,
runs only known transactional migrations and checks integrity before returning.
Complete corruption and unknown formats fail closed with redacted errors.

Internal schema V2 gives `SqliteSyncOutbox` bounded paging, payload-free stats,
durable attempt/readiness metadata and transactional ordered partial receipts.
Page metadata is selected before BLOB materialization. A known V1 database is
migrated atomically; rollback requires the verified pre-migration backup.

One store creates independent handles for:

- `SqliteReplicationLog`;
- `SqliteSyncOutbox`;
- `SqliteSyncCheckpointStore`;
- `SqliteSyncTombstoneStore`.

Clones share a pool admitting at most 32 connections. Writer admission and
SQLite busy waits have deadlines. Log reads, snapshots, outbox entries,
tombstones, database pages and backup steps have explicit limits.

Outbox admission computes the exact canonical JSON byte length without an
encoded copy and streams the record into an incremental SQLite BLOB. Duplicate
checks, page reads and startup validation stream the BLOB as well, so the owned
message never shares memory with a second record-sized encoded buffer. Reader
and writer scratch follows the encoded record size and is capped at 64 KiB and
1 MiB respectively; small records do not reserve maximum buffers.

Portable snapshots use `ReplicationSnapshot` V1. Online backup uses SQLite's
backup API and publishes only a verified new file. Restore also targets a new
path; replacing a live database is intentionally unsupported. Keep a database,
its `-wal` file and its `-shm` file together until all handles close.

Snapshot creation transfers database payload allocations into the portable
value. Portable restore validates that value through a shared reference,
rejects aggregate payload bytes above `max_database_bytes` before deleting any
row, then borrows the records during its single replacement transaction. The
32 MiB restore workload measured 396.00 ms p50 and 73.84 MiB peak RSS on Apple
M1, versus 466.80 ms and 108.97 MiB with two temporary payload replicas.

SQLite supports independent local processes on a filesystem with reliable
locking. Network shares and concurrent hosts are outside this profile. The
provider contains no application schema and offers no arbitrary SQL escape.

For rollback, stop admission, drain/export the outbox, create a verified backup
and export a portable replication snapshot. File persistence must be created
explicitly from public artifacts; database renaming is not a migration.

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

## Certification

The clean release benchmark at `0f6f6d0` passed on macOS arm64 with Rust
1.97.1. For 2,048 durable 1 KiB appends and 2,048 point reads, append p99 was
1.086 ms at 3,729 operations/s and read p99 was 0.583 ms at 6,578 operations/s.
A verified 3,182,592-byte online backup took 73.870 ms; the full integrity scan
took 15.675 ms. Reproduce it with `appcore-certification bottlenecks` as
documented in `release/sqlite-sync-provider-v1.md`. In the current 512-entry
small-record outbox workload, enqueue requested 255,676 Rust heap bytes with no
retained growth and 141,791 ns p99, under the explicit 2 MiB allocation and
250 ms latency gates. Right-sized BLOB scratch reduced requested bytes for the
complete SQLite workload from 578,081,344 to 8,251,670 (-98.57%) and peak live
heap delta from 1,083,528 to 233,600 bytes (-78.44%).

The hardware-aware crate runner also exercises large data paths. On Apple M1,
macOS 27, three measured processes and one warmup, a raw 16 MiB outbox enqueue
had p50 249.76 ms, 42.81 MiB peak RSS and a 19.62 MiB workload RSS delta. A
32-record/32 MiB snapshot restore had p50 361.29 ms, 73.95 MiB peak RSS and a
2.55 MiB workload delta; its prepared snapshot was already present at the idle
checkpoint. The non-versioned report is
`target/appcore/bench/sync-sqlite-memory.json`.
