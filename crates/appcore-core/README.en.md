# appcore-core

The runtime benchmark compares current redaction with the preserved algorithm
from `efe9a205`: `redaction_plain_8192_{current,reference}` and
`redaction_mixed_{current,reference}`. Fixture creation and output equality checks
are outside timing; each iteration includes redaction and result release.
Both implementations run in the same optimized binary, with separate process
samples. These cases measure diagnostic text, not whole-journal I/O or an
allocation count. The reference exists only in the benchmark.

Redaction checks whether the complete text is already safe and bounded before
running marker passes; that path allocates only the owned result. Otherwise,
passes retain the previous lowercase substring search but reuse the current
output when a marker is absent. Direct candidate-search variants were rejected
because paired benchmarks exposed regressions on mixed text.
The seven marker passes retain their original order, delimiters and UTF-8
truncation behavior. Public results remain owned strings; matching passes may
allocate, and no throughput or process-RSS improvement is asserted without a
paired benchmark. This remains conservative marker redaction, not a parser
that can identify every secret in arbitrary payloads.

Decision registries and engines admit at most 4,096 unique names of 1–256
UTF-8 bytes. Invalid, duplicate or excess registration fails before retention
and preserves existing evaluation order. This bounds registry metadata, not
memory owned internally by application-provided decision nodes.

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Generic in-process Runtime lifecycle, command/event dispatch, state, decisions,
audit and idempotency.

**Responsibility:** generic in-process lifecycle, registration, dispatch,
state, audit and idempotency.

**Internal dependencies:** `appcore-contracts`, `appcore-types`.

**Main API:** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, command/event registries and buses, envelopes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log/journal,
in-memory/file idempotency, state and decision engines, clock, redaction and
the compatibility `AppPlugin`.

`RuntimeBuilder` and `AppPlugin` remain low-level compatibility contracts. New
applications use `appcore_sdk::Application` and the SDK's small local `run`
entry point; deployments select process composition explicitly.

`RuntimeController` clones share lifecycle, idempotency and in-flight command
state. The immutable command bus owns handlers through `Arc`. Independent
handlers may execute concurrently, while one idempotency key admits at most one
execution. Shutdown closes admission atomically and can wait for already
admitted commands through a bounded drain.

`RuntimeLifecycle` stores one `Copy` state enum under its mutex and applies the
exact 12 stable edges through a total transition function. It allocates no
validated names or transition table per instance. The generic public
`StateMachine` remains available and unchanged for application-owned states.

File idempotency is scanned incrementally. The V1 journal is limited to 64 MiB,
each record to 1 MiB, 131,072 persisted records between compactions and 65,536
active keys. Append compacts atomically before a journal limit is crossed;
startup discards one bounded incomplete final record without accepting corrupt
complete records. The file store retains only keys plus verified offsets and
loads one response body on demand; it does not retain all replay bodies in the
heap. In-memory idempotency uses the same active-key ceiling.

`FileOperationalJournal` also scans one bounded line at a time and rejects a
record above 1 MiB. Hashing, append and atomic compaction serialize directly to
counters, digests and files instead of building complete JSON buffers or
cloning retained records. `write_audit_jsonl` streams export to a caller-owned
writer; `export_audit_jsonl` remains the compatible owned-string adapter.

The process-local `AuditLog` has a 16 MiB aggregate default budget across its
command and generic-entry snapshots, in addition to the 10,000-item ceilings.
`with_max_bytes` may tighten it, `stats` exposes current/peak bytes, evictions
and rejections, and `write_jsonl` serializes a shared copy-on-write snapshot to
the caller's writer without holding the log lock during I/O. Cloning the log
shares immutable snapshots until either clone mutates.

Use `entries_snapshot` when a bounded structured export needs a JSON array. It
captures the shared immutable queue without cloning entry fields and implements
`Serialize`; later log mutations do not change the captured view. The
10,000-entry, 2,996,676-byte pretty-JSON benchmark measured 1.12 ms p50 and
6.42 MiB peak RSS on Apple M1.

`records_snapshot` provides the same contract for command records. Both
snapshot types expose `recent(limit)` so a bounded query can borrow only its
newest page after releasing the log lock. A 1,000-item tail over 10,000 records
and entries measured 2.06 us p50 and 11.88 MiB peak RSS, versus 4.16 ms and
20.33 MiB for the compatible full-copy methods.

With an attached `FileOperationalJournal`, newly appended audit entries and
safe restored entries share one immutable operational record allocation with
the journal. Journal load validates the hash chain, checks audit text without
allocating, sanitizes only unsafe content and atomically rewrites that content
before exposing it. Later log attachment therefore copies only bounded `Arc`
handles. Public owned accessors, snapshot JSON and the V1 journal format remain
unchanged. A one-attachment workload over 384 safe entries (about 3 MiB) reduced
p50 from 12.26 ms to 86.50 us (-99.29%), peak RSS by 0.57% and workload RSS by
1.72% on Apple M1. The separate real-fsync workload retained its earlier
27.83% p50, 37.30% peak-RSS and 47.93% retained-memory improvements.

The process-local `EventBus` likewise retains at most 10,000 events and 16 MiB
by default. `with_max_bytes` tightens that ceiling, `stats` reports byte
pressure, evictions and oversized-event rejections, and `snapshot().recent`
borrows a stable newest page. Selecting 1,000 of 10,000 events measured 2.39 us
p50 and 8.48 MiB peak RSS, versus 2.09 ms and 14.59 MiB for `events()`.
When a `FileOperationalJournal` is attached, the bus and journal retain the
same immutable event record allocation. Restore copies only bounded `Arc`
handles; the public owned APIs and the V1 journal format are unchanged. A
3 MiB retained-event workload reduced peak RSS from 8.11 to 5.08 MiB (-37.38%)
and retained workload memory by 48.00%, with disk-dominated p50 within 0.95%.

This crate contains no product domain, HTTP server or provider-specific code.

**Maturity:** stable low-level RC surface; builder/plugin remain compatibility
contracts and manifest-first is the preferred application path.

```bash
cargo test -p appcore-core
```

## Stable documentation

Stable ID: **ACR-008**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-008). This permanent ID
remains valid if the wiki page moves.
