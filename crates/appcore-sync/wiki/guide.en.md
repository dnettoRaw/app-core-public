# appcore-sync

The next-major observation contract is fallible: `ReplicationLog::len`,
`last_index` and `is_empty` return `SyncResult`. Treat an error as unknown
persistence health; never replace it with zero or a cached value. Migration and
rollback are documented in
[`release/fallible-replication-log-observations.md`](../../../release/fallible-replication-log-observations.md).

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** conservative leader-to-follower replication contracts and
local durability helpers.

**Internal dependencies:** `appcore-core`, `appcore-distributed-contracts`,
`appcore-ops`, `appcore-transport`.

**Primary API:** node role/status/peer/heartbeat and `SyncMessage`; V1 wire
codec; replication logs and snapshots; in-memory/file checkpoints and outbox;
receiver state/acknowledgement; follower client; HTTP transport; peer discovery;
retry policy, push metrics and `SyncError`.
Opaque content-envelope transport contracts are reexported for DNT-backed sync
packages without exposing plaintext to routing code.

`HttpSyncTransport` owns a reusable bounded HTTP client. Use
`with_timeout_ms` for the uniform V1 deadline or `with_timeouts` for independent
connect/admission, read and write deadlines.

Use it for compatible, ordered, hash-chained replication. Do not bypass
identity/protocol checks or reinterpret it as RAFT, multi-master consensus or a
business conflict resolver.

The file log is capped at 256 MiB and the outbox at 64 MiB. Checkpoint peer IDs
and hashes are validated on write and load. A receiver validates the complete
batch, sequence arithmetic and every record bound before any log or checkpoint
mutation, so a late invalid event cannot leave a partial append.

The next-major file outbox is the explicit V2 append-only binary journal.
Enqueue and acknowledgement append and sync one ordinal/hash-chained frame;
current instances scan only new tail bytes. Atomic compaction changes the
generation and retains pending records. Startup truncates only an incomplete
final frame and fails closed on complete corruption, duplicates, reordering or
unsupported versions. V1 is never inferred or converted: drain V1 before an
upgrade and V2 before rollback, following the
[migration runbook](../../../release/outbox-v2-migration.md).

**Maturity:** stable conservative RC profile with strict V1 decoding.
