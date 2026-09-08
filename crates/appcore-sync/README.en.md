# appcore-sync

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Conservative leader-to-follower replication with versioned wire, log,
snapshots, checkpoints, outbox, receiver and transport contracts.

**Responsibility:** conservative leader-to-follower replication and local
durability helpers.

**Internal dependencies:** `appcore-core`, `appcore-distributed-contracts`,
`appcore-ops`, `appcore-transport`.

**Main API:** node role/status/peer/heartbeat and `SyncMessage`; V1 wire codec;
replication logs/snapshots; memory/file checkpoints and outbox; receiver
state/acknowledgement; follower client; HTTP transport; peer discovery; retry,
metrics and `SyncError`.

Identity, protocol, sequence and hash-chain validation are mandatory. This
crate is not RAFT, multi-master consensus or a domain conflict resolver.
Reexported opaque-envelope contracts expose the 1,024-byte
`MAX_OPAQUE_MESSAGE_ID_BYTES` retention bound.

File logs, snapshots, checkpoints and outbox records are versioned and bounded.
The receiver validates the complete incoming batch, sequence range and record
sizes before mutating the replication log or checkpoint.

`FileSyncCheckpointStore` scans V1 through a fixed 16 KiB reader. Startup
validation retains no peer map, and lookup validates the complete file while
owning only the requested hash. Mutation still builds the canonical sorted map
once, but writes it directly through a fixed buffer instead of keeping an
additional file-sized `String`. The public ceilings are 8 MiB, 65,536 non-empty
records and 256 UTF-8 bytes per peer ID; an encoded line is bounded before it
can grow the scratch buffer. Duplicate peers keep the last value as before and
the next successful mutation canonicalizes them.

`FileReplicationLog` scans V1 incrementally and retains only a compact sorted
vector of sequence/record-index pairs plus per-record offsets, lengths and
digests, not every payload. The in-memory implementation uses the same flat
sorted index and binary-search lookup, avoiding hash-bucket overhead for bounded
local logs. Concurrent instances validate
the hash-chain anchor and scan only a new tail; atomic snapshot replacement is
treated as a new generation and rebuilt safely. `events_page` limits reads to
1,024 records and 48 MiB before allocation. The complete `events_since` remains
source-compatible but rejects a file read above those limits. Deployment-owned
sync tools should use pages; no Runtime sync CLI is shipped. The log remains
capped at 256 MiB, each payload at 1 MiB and the
index at 262,144 records. Runtime HTTP batches additionally stop at 1 MiB of raw
events; the encoded V1 JSON envelope is capped at 5 MiB so even the worst-case
numeric byte representation of one valid payload remains transferable.
The V1 encoder serializes borrowed identity, message and event fields directly
into its required output string. It does not clone the complete batch before
encoding, and its JSON remains byte-identical to the owned V1 contract.

`ReplicationSnapshot::try_from_records` consumes owned sequence/payload pairs
and moves their allocations into a checksum-protected V1 snapshot.
`ReplicationSnapshot::validate` checks the same format, count, payload,
sequence and checksum invariants through a shared reference, without creating
a second payload collection. Persistent providers can therefore validate
before mutation while retaining one semantic snapshot owner.
Memory-backed consumers that own the snapshot can call
`InMemoryReplicationLog::restore_snapshot_owned` to validate and move payloads
directly into the log, avoiding simultaneous snapshot and destination copies.

In `1.0.2-rc`, `ReplicationLog::len`, `last_index` and
`is_empty` return `SyncResult`. Persistent providers surface observation
failures instead of substituting zero or stale state. Consumers must handle the
result before updating; see
[`release/fallible-replication-log-observations.md`](../../release/fallible-replication-log-observations.md).

The `1.0.2-rc` `FileSyncOutbox` uses the explicit
`appcore-sync-outbox-v2` append-only binary journal. Enqueue and acknowledgement
sync one integrity-chained frame; readers scan only a new tail, and bounded
compaction atomically retains pending messages. Only an incomplete final frame
is recoverable. A complete corrupt, V1, unversioned or future-format file fails
closed. Drain the V1 queue before upgrading and the V2 queue before rollback;
see [`release/outbox-v2-migration.md`](../../release/outbox-v2-migration.md).

The resident file-outbox state contains only batch IDs, journal offsets,
encoded lengths, payload digests and retry metadata. Enqueue measures JSON in a
bounded pass and then writes it through a fixed 64 KiB buffer; it does not keep
an encoded copy. Front and page reads decode and verify one indexed message at
a time. Each batch ID is shared with transactional tail-scan state, so refresh
clones only handles instead of copying every pending identifier. The
source-compatible `messages()` snapshot still returns an owned
`Vec`, so memory-sensitive consumers must use bounded pages.

`InMemorySyncOutbox` also obtains the exact encoded length with an
overflow-checked JSON counter rather than allocating and discarding a complete
encoded message. A valid 4 MiB batch measured 10.92 ms p50 and 17.52 MiB peak
RSS on Apple M1, down from 23.55 ms and 45.73 MiB with the temporary buffer.

The receiver's 10,000-entry processed-batch window retains one shared
allocation per `batch_id` across duplicate lookup and acceptance-order
eviction. Applying 10,000 batches with 128-byte IDs measured 58.27 ms p50 and
15.27 MiB peak RSS on Apple M1, down from 61.29 ms and 17.45 MiB with duplicate
strings. Receiver and outbox boundaries reject empty IDs, control characters
and IDs above 1,024 UTF-8 bytes before retaining them. This keeps the fixed
entry window bounded by bytes as well as count. Duplicate rejection and
oldest-first eviction are unchanged.

The additive `1.0.2-rc` `SyncOutbox` paging contract exposes `peek`, `stats`,
`mark_attempt`, `next_ready` and ordered partial receipts. Page reads are capped
at 1,024 messages and 48 MiB before payload clones. The in-memory provider
and file providers implement exact paging and retry observations. File attempts
and ordered receipts are hash-chained journal frames that survive restart. A
provider can call `encoded_sync_message_bytes` to obtain the exact compact JSON
size without an encoded copy, then call `write_sync_message_json` to stream that
same canonical representation to a bounded writer. String escaping remains
Serde-compatible and event bytes are emitted through a fixed 16 KiB scratch
buffer. A receipt is measured and serialized directly through the fixed 64 KiB
writer;
the maximum escaped-ID fixture no longer materializes its 2,086,913-byte JSON
buffer in production. Journal scans borrow unescaped IDs from the frame. An
external provider relying on the compatibility defaults still compiles: it
returns at most the front message, reports unknown extended statistics and
rejects persisted attempts or multi-message receipts explicitly.

`FollowerSyncClient` uses this bounded contract
directly. Each failed transport call records retry readiness, success applies
an exact receipt, and draining exposes the last acknowledged batch for
checkpoint progress. The complete `pending_messages` snapshot remains for
source compatibility; new consumers should use `pending_page` and
`outbox_stats`.

`HttpSyncTransport` owns a reusable bounded HTTP client. `with_timeout_ms`
keeps the uniform V1 deadline, while `with_timeouts` selects independent
connect/admission, read and write deadlines.

```bash
cargo test -p appcore-sync
```

The default `ReplicationLog::events_page` is a full-read adapter for external
providers: it validates limits, calls `events_since`, then moves selected
payloads into the bounded result. It does not bound that initial read. Providers
must override paging to enforce record/byte limits before reads or clones;
internal memory/file providers already do so. A smaller returned page alone
is not proof of bounded materialization. The consumer regression test is
`cargo test -p appcore-sync --test external_log_paging`.

**Maturity:** stable conservative RC profile with strict V1 decoding.

## Stable documentation

Stable ID: **ACR-012**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-012). This permanent ID
remains valid if the wiki page moves.
