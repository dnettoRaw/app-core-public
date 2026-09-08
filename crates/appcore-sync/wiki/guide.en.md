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
packages without exposing plaintext to routing code. Their public
`MAX_OPAQUE_MESSAGE_ID_BYTES` retention limit is 1,024 UTF-8 bytes.

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

The checkpoint V1 file is capped at 8 MiB and 65,536 non-empty records, with
peer IDs capped at 256 UTF-8 bytes. `FileSyncCheckpointStore` validates one
bounded line at a time through a fixed 16 KiB reader. Startup retains no parsed
map; a lookup scans and validates the complete file but allocates only if its
target is present. A mutation builds the sorted canonical map once and streams
it to the atomic temporary file, so no complete input or output `String`
coexists with that map. Duplicate entries retain the last value, preserving V1
behavior, and the next mutation writes one canonical entry.

`FileReplicationLog` scans one bounded V1 line at a time and retains only a
sorted compact sequence-to-record index plus offsets, lengths and digests. The
in-memory log uses the same flat sorted index and binary-search lookup, so
bounded local logs do not retain hash buckets.
Payloads are decoded on demand, one at a time. A locked append validates the last hash-chain anchor and scans only bytes
added by another instance; an atomic snapshot replacement invalidates that
anchor and triggers a complete incremental index rebuild. Use
`events_page(index, max_records, max_bytes)` with ceilings of 1,024 records and
48 MiB. The complete compatibility method rejects larger file reads. Deployment
tools should use the paged method; no Runtime sync CLI is shipped. File,
payload and record-count ceilings are
256 MiB, 1 MiB and 262,144. Runtime HTTP pages use at most 1 MiB of raw events
inside a 5 MiB encoded V1 envelope, including the worst-case JSON byte-array
expansion.
The wire encoder borrows the identity, message and every event while writing
that required output string. It therefore avoids a second complete batch in
memory and preserves the exact owned V1 JSON encoding and source-node check.

Build portable snapshots from already-owned payloads with
`ReplicationSnapshot::try_from_records`; it transfers each `Vec<u8>` into the
snapshot. Providers call `ReplicationSnapshot::validate` through `&self` to
check version, count, per-record size, unique non-zero sequences and checksum
without cloning the payload collection. Validation must finish before a restore
transaction changes durable state.
An in-memory consumer that owns the snapshot may use
`InMemoryReplicationLog::restore_snapshot_owned` to validate and move payloads
into the log without retaining both collections.

The `1.0.2-rc` file outbox is the explicit V2 append-only binary journal.
Enqueue and acknowledgement append and sync one ordinal/hash-chained frame;
current instances scan only new tail bytes. Atomic compaction changes the
generation and retains pending records. Startup truncates only an incomplete
final frame and fails closed on complete corruption, duplicates, reordering or
unsupported versions. V1 is never inferred or converted: drain V1 before an
upgrade and V2 before rollback, following the
[migration runbook](../../../release/outbox-v2-migration.md).

The in-process V2 index retains only the batch ID, ordinal, data offset,
encoded length, payload digest and retry metadata. Enqueue first measures and
hashes JSON, then serializes the same message directly through a fixed 64 KiB
buffer. `front`, `peek` and `next_ready` seek to the indexed record, decode one
message and verify its exact length and digest. This makes the journal the
payload owner without making it the source of delivery order or retry state.
The batch ID is an `Arc<str>` shared by the live index and transactional tail
scan. A refresh therefore clones only handles, not up to 1,024 identifier bytes
for every pending message; newly scanned enqueue state shares the same
allocation with its pending operation.
Use paged methods when memory is bounded: the compatibility `messages()` method
must materialize every requested message because its public result is a `Vec`.

The in-memory provider measures that same exact JSON length analytically with
overflow-checked arithmetic. `encoded_sync_message_bytes` provides this count
to integration providers, while `write_sync_message_json` streams the identical
Serde-compatible representation through a fixed 16 KiB event scratch buffer.
Neither path creates a second complete encoded message merely to decide
admission, page boundaries, persistence or `pending_bytes`. For a
valid 4 MiB batch on Apple M1, p50 fell from 23.55 ms to 10.92 ms and peak RSS
from 45.73 MiB to 17.52 MiB.

The receiver also stores one shared allocation per processed `batch_id` across
its duplicate-membership set and ordered eviction queue. Its window remains
fixed at 10,000 IDs. Applying 10,000 batches with 128-byte IDs on Apple M1
measured 58.27 ms p50 and reduced peak RSS from 17.45 MiB to 15.27 MiB, without
changing duplicate rejection or oldest-first eviction. Receiver and outbox
boundaries reject an empty ID, control characters or more than 1,024 UTF-8
bytes before retaining the message. `SyncMessage::new` remains an infallible
data constructor; acceptance is decided at these stateful boundaries.

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
incomplete final frame retains the unacknowledged prefix. Receipt JSON is
measured first and then serialized directly through the fixed 64 KiB writer.
The maximum 1,024-ID escaped fixture is 2,086,913 bytes, which is no longer held
as one additional production `Vec`. Scanning borrows IDs without JSON escapes
from the existing frame and allocates identifier strings only when unescaping
is required.

The follower drives `next_ready`, `mark_attempt` and exact receipts directly.
Use `pending_page`, `outbox_stats` and `flush_pending_with_progress` for bounded
inspection and checkpoint progress. Runtime delivery never calls the complete
compatibility snapshot.

The default `ReplicationLog::events_page` is a full-read adapter for external
providers: it validates limits, calls `events_since`, then moves selected
payloads into the bounded result. It does not bound that initial read. Providers
must override paging to enforce record/byte limits before reads or clones;
internal memory/file providers already do so. A smaller returned page alone
is not proof of bounded materialization. The consumer regression test is
`cargo test -p appcore-sync --test external_log_paging`.

**Maturity:** stable conservative RC profile with strict V1 decoding.
