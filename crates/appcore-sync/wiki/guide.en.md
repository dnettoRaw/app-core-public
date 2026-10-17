# appcore-sync

Release candidate **`appcore-sync 1.0.2-rc.1`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.


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

For UI-ready conflict handling, use `SyncConflict` and
`InMemorySyncConflictStore`. Records contain only bounded peer/sequence,
SHA-256 and typed-reason metadata. Resolution requests are idempotent and a
different decision cannot overwrite an existing one; domain payload merging
remains outside this crate.

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

For resumable opaque transfers, use `split_sync_payload` and feed the returned
chunks to `SyncChunkAssembler` in any order. Resume from the payload-free
`progress().missing` ranges and call `assemble` only after all bytes arrive.
The adapter bounds sizes, verifies both SHA-256 levels, accepts identical
replays and rejects conflicting overlaps without changing V1.

[Application envelope migration](../../appcore-peer-rpc/wiki/transport-migration.en.md).

## Opaque revision causality

This section describes unpublished source changes. Verify the exact registry package before consumer adoption.

`ObjectRevisionV1`, `RevisionChainV1` and `decide_object_revision_v1` classify `already_applied`, `ancestor_echo`, `applicable`, `concurrent_branch` and `snapshot_required`. The chain contains at most 1,024 revisions with 128-byte identity fields. Authenticate every event and bind its tenant before constructing evidence: chain validation is structural, not cryptographic. Missing links, cycles, object mismatch and reused event/revision identities fail closed. Equality of content or timestamps never establishes ancestry. An intentional revert uses a fresh revision/event. Advance the transport checkpoint independently when accepting an ancestor echo; call `validate_checkpoint_head_v1` before mutating metadata.

## Retransmission and checkpoint serialization

Unpublished source fix: an identical authenticated replay returns an accepted ACK with zero new records; divergent reuse of a retained `batch_id` fails with `divergent batch_id reuse`. The receiver retains at most 10,000 ID/digest pairs. This window is process-local, not permanent replay evidence; after restart or eviction the existing old-sequence checkpoint policy applies. Authenticate and isolate the source before calling the receiver. Distinct receiver states must share the same replication-log handle when they share checkpoints.

Checkpoint observation, overlap verification, log append and sequence/hash publication share the log lock. Concurrent batches cannot publish a stale smaller checkpoint or combine one batch’s sequence with another batch’s hash. Provider errors can still leave an appended log with an older checkpoint; an identical retry verifies overlap before publishing the checkpoint. The sender removes its outbox receipt only after transport acknowledgement. Losing the first ACK therefore causes a safe retransmission and eventual drain. External processes and independent log handles require provider/deployment transaction coordination.

## Release candidate — 2026-10-14

`2.0.0-alpha.5`: source changes.

## Canonical service history (unpublished source)

`sync::canonical::FileCanonicalHistoryV1` preserves original `SyncMessage` metadata and event bytes, with transferable `CanonicalBatchV1` epoch/writer evidence. `CanonicalStreamV1` identifies tenant/cluster/service independently of the current Core or address. `CanonicalHeadV1` binds sequence, original batch hash and a second digest over stream, epoch, writer and history. Existing V1 receiver, wire, log and snapshot formats are unchanged; there is no automatic import or conversion.

Use explicit `create` only for initial provisioning; restart uses `open_existing`. Every operation reloads and validates durable state under an OS try-lock. Writes sync an exclusive temporary file, atomically rename it and sync the directory where supported. Lost ACKs accept identical persisted records after restart; gaps, divergent IDs/overlaps, backwards epochs and changed writers within one epoch fail closed. Corruption or missing storage never creates a new history. A failed persistence transaction must be reopened and its result inspected: rename may already have succeeded.

`batches(offset, limit)` returns up to 64 complete lineage records; authenticate the entire record before `replicate_authenticated`. This copies canonical history only, not application materialization or effect transactions. Historical epoch evidence is not a current lease. The host must authenticate tenant/service/source and historical authority before admitting records; hashes are not signatures.

`with_verified_head(required, action)` compares the complete durable head and holds the history lock during the callback. The required head must come from a trusted durability policy, not the candidate itself. An empty or stale replica, even at the same sequence with a different hash, cannot run the action. Inside the callback compose a current exact authority fence around writer commit, using a consistent history-then-authority lock order. The co-located reference is `FileControlPlane::with_fenced_service_write`; do not reenter either provider. A remote `ServiceLeaseAuthority::check` is an observation, not an atomic remote commit. The primitive does not promote a node or issue a readiness certificate by itself.

Discover the writer through `ServiceLeaseAuthority::inspect` for the same tenant/cluster/service, then resolve its Core through deployment discovery and authenticate the endpoint. Inspection may return an expired grant. A former writer rejoins as a replica; divergent history is rejected for explicit operator/application snapshot reconciliation. Do not reset sequence or rename the source to hide divergence. An external rollback of both authority and history is outside this reference; certified storage custody remains mandatory.

Limits: 16 MiB encoded file, 1,024 batches, 16,384 events and 4 MiB total raw payload, each event at most 1 MiB. Identity fields use a 128-byte ASCII alphanumeric/`-_.` profile. This small reference reloads the whole bounded file; JSON decoding has allocation overhead before decoded admission, so it is not a large-log streaming provider. Run filesystem operations on bounded blocking workers. Capacity exhaustion stops writes; no evidence is evicted or compacted. Application snapshots, replica-confirmation policy and physical mesh qualification remain separate deployment work.

Each successful append commits immediately; returning an error from the callback does not roll back an earlier append. Reopen and inspect the durable head before retrying an uncertain result.

## Canonical-copy durability (unpublished source)

`DurabilityTargetV1` fixes the original batch ID, complete head, writer Core and epoch. `DurabilityPolicyV1` explicitly selects `local`, `master` or `master_and_replica`. `evaluate_durability_v1` returns `DurabilityStatusV1` with `confirmed`, `persisted_core_ids` and `missing_core_ids`; pending never silently downgrades the policy. Master means the original writer for this immutable target, not whichever Core discovery currently returns. A retry keeps the same target and policy; pending/timeout does not roll back a write or authorize rerunning an application handler.

| Policy | Required canonical copies | Remaining loss exposure |
| --- | --- | --- |
| `Local { core_id }` | Selected local Core | Loss of that copy; no master/replica confirmation implied |
| `Master` | Original writer Core | Loss of the writer's storage before another confirmed copy |
| `MasterAndReplica { replica_core_id }` | Original writer and one distinct selected replica Core | Loss of both copies or shared failure domain; distinct IDs alone do not prove independent disks |

`FileCanonicalHistoryV1::durability_target` recovers an exact retained batch boundary. `durability_receipt(target, local_core_id)` reloads and validates retained bytes under the history lock, verifies all target fields, syncs the file and directory where supported, then issues a deterministic V1 receipt. Bind `local_core_id` to deployment-owned storage identity, never a requester field. An older target remains confirmable only as an exact retained ancestor, allowing lost-reply recovery after restart and later batches without duplicate append. No separate receipt journal or history-format change is needed. Genesis, partial batch boundaries, missing/corrupt/busy storage and divergent targets cannot issue a receipt.

Authenticate the entire receipt and bind `persisted_by_core_id` to the authenticated sender before evaluation. JSON decoding and digests are not authentication. `decode_durability_receipt_v1` bounds input to 16 KiB and rejects unknown/unversioned input with `NO MORE SUPPORTED PLEASE UPDATE`; evaluation admits at most 16 receipts. Repeated evidence from one Core is idempotent and never counts twice. Every receipt must match the complete target; unexpected Cores cannot satisfy selected requirements. Receipt fields are not a current lease, availability probe, application materialization proof or consensus certificate. Filesystem sync remains subject to the platform/filesystem's certified durability guarantees.

The deployment persists the selected policy, trusted target and authenticated confirmations in its recovery boundary before reporting success or retiring the canonical batch. It must serialize outbox mutations and bind retirement to the same batch content, not merely an ID check racing another consumer. These primitives retain no unbounded pending-operation map and do not automatically mutate an outbox. The existing `FollowerSyncClient` keeps its transport-ACK contract; applications requiring replica durability must explicitly compose this confirmation path. The integration test retains a real file outbox while pending, restarts it after a lost replica reply and retires only after both matching receipts arrive.

These policies confirm **canonical history copies**, not a pre-canonical local application transaction/outbox. Existing local-first offline application durability remains application-owned. Do not label a mere local outbox entry as master/replica confirmation. A trusted confirmed target can supply the required head for promotion checks only when the deployment retains it and authenticates its evidence; current exact leadership and atomic write fencing remain mandatory. No production integration, hardware power-loss qualification or zero-loss guarantee is asserted.
