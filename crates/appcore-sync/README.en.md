# appcore-sync

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Conservative leader-to-follower replication with versioned wire, log,
snapshots, checkpoints, outbox, receiver and transport contracts.

Identity, protocol, sequence and hash-chain validation are mandatory. This
crate is not RAFT, multi-master consensus or a domain conflict resolver.

File logs, snapshots, checkpoints and outbox records are versioned and bounded.
The receiver validates the complete incoming batch, sequence range and record
sizes before mutating the replication log or checkpoint.

In the 1.5 candidate, `ReplicationLog::len`, `last_index` and
`is_empty` return `SyncResult`. Persistent providers surface observation
failures instead of substituting zero or stale state. Consumers must handle the
result before updating; see
[`release/fallible-replication-log-observations.md`](../../release/fallible-replication-log-observations.md).

The 1.5 candidate `FileSyncOutbox` uses the explicit
`appcore-sync-outbox-v2` append-only binary journal. Enqueue and acknowledgement
sync one integrity-chained frame; readers scan only a new tail, and bounded
compaction atomically retains pending messages. Only an incomplete final frame
is recoverable. A complete corrupt, V1, unversioned or future-format file fails
closed. Drain the V1 queue before upgrading and the V2 queue before rollback;
see [`release/outbox-v2-migration.md`](../../release/outbox-v2-migration.md).

The additive 1.5 candidate `SyncOutbox` paging contract exposes `peek`, `stats`,
`mark_attempt`, `next_ready` and ordered partial receipts. Page reads are capped
at 1,024 messages and 48 MiB before payload clones. The in-memory provider
and file providers implement exact paging and retry observations. File attempts
and ordered receipts are hash-chained journal frames that survive restart. A
pre-1.5 external provider still compiles through conservative defaults: it
returns at most the front message, reports unknown extended statistics and
rejects persisted attempts or multi-message receipts explicitly.

`FollowerSyncClient` and the Runtime sync CLI use this bounded contract
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
