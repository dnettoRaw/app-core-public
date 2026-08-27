# appcore-sync

The `1.0.2-rc` observation contract is fallible: `ReplicationLog::len`,
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

The `1.0.2-rc` file outbox is the explicit V2 append-only binary journal.
Enqueue and acknowledgement append and sync one ordinal/hash-chained frame;
current instances scan only new tail bytes. Atomic compaction changes the
generation and retains pending records. Startup truncates only an incomplete
final frame and fails closed on complete corruption, duplicates, reordering or
unsupported versions. V1 is never inferred or converted: drain V1 before an
upgrade and V2 before rollback, following the
[migration runbook](../../../release/outbox-v2-migration.md).

The `1.0.2-rc` outbox extension pages with `peek(limit, max_bytes)`, reports
payload-free `stats`, records retry readiness with `mark_attempt`, selects only
the ordered ready prefix with `next_ready` and applies exact partial-prefix
receipts. Global page ceilings are 1,024 messages and 48 MiB. Compatibility
defaults never call `messages()`: pre-extension providers expose one immediate
front message, unknown extended statistics and explicit unsupported errors for
state they cannot persist.

`FileSyncOutbox` records each front-message attempt and each validated receipt
as a bounded hash-chained V2 journal frame. Retry counters/readiness survive a
restart; a complete corrupt attempt or receipt fails closed, while an
incomplete final frame retains the unacknowledged prefix.

The follower drives `next_ready`, `mark_attempt` and exact receipts directly.
Use `pending_page`, `outbox_stats` and `flush_pending_with_progress` for bounded
inspection and checkpoint progress. Runtime delivery never calls the complete
compatibility snapshot.

**Maturity:** stable conservative RC profile with strict V1 decoding.
