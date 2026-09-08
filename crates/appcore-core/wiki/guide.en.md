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

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** generic in-process Runtime lifecycle, registration,
dispatch, state, audit and idempotency.

**Internal dependencies:** `appcore-contracts`, `appcore-types`.

**Primary API:** `RuntimeBuilder`, `RuntimeController`, `RuntimeInstance`,
`RuntimeLifecycle`, command/event registries and buses, envelopes,
`CommandHandler`, `CommandResult`, `RuntimeContext`, audit log and journal,
in-memory/file idempotency, state and decision registries/engines, clock,
redaction and compatibility `AppPlugin`.

Cloned `RuntimeController` values share lifecycle, idempotency and in-flight
command state. The immutable command bus owns handlers through `Arc`.
Independent handlers may execute concurrently, while one idempotency key admits
at most one execution. Request shutdown before calling the bounded in-flight
drain; new commands are then rejected without racing the lifecycle transition.

`RuntimeLifecycle` stores one `Copy` state enum under its mutex and applies the
exact 12 stable edges through a total transition function. It allocates no
validated names or transition table per instance. The generic public
`StateMachine` remains available and unchanged for application-owned states.

`FileIdempotencyStore` scans its V1 journal one bounded line at a time instead
of materializing the file. The file/record/active-key limits are 64 MiB, 1 MiB
and 65,536; at most 131,072 journal records are retained between atomic
compactions. A bounded incomplete final line is recovered, while malformed
complete lines fail closed. Its resident map contains keys and SHA-256-verified
offsets, not response bodies; `get` reads only the selected bounded record.
`InMemoryIdempotencyStore` shares the active-key limit so both implementations
have an explicit memory ceiling.

`FileOperationalJournal` uses the same incremental boundary discipline for
audit and event persistence. Startup holds at most one 1 MiB record line while
validating the hash chain. Append hashes the serialized record through a
bounded counter and digest writer, and compaction finds the largest retained
suffix that fits before streaming one atomic replacement. Use
`write_audit_jsonl` for a bounded caller-owned export sink; the existing
`export_audit_jsonl` intentionally materializes the requested output string.

The in-memory `AuditLog` separately bounds its two snapshots by 10,000 items
and one shared 16 MiB default budget. Use `with_max_bytes` to tighten that
budget, inspect `stats` for current/peak pressure, evictions and rejections,
and prefer `write_jsonl` to stream a copy-on-write snapshot after releasing the
state lock. `export_jsonl` remains the compatible owned-string adapter.

For a structured JSON array, call `entries_snapshot`. The returned immutable
view implements `Serialize`, shares entry storage instead of deep-cloning it,
and remains stable if the live log changes. A 10,000-entry, 2,996,676-byte
pretty-JSON workload measured 1.12 ms p50 and 6.42 MiB peak RSS on Apple M1.

`records_snapshot` is the corresponding command-record view. Both snapshots
offer `recent(limit)` for a borrowed newest page after the state lock is
released. Selecting 1,000 of 10,000 records and entries measured 2.06 us p50
and 11.88 MiB peak RSS, versus 4.16 ms and 20.33 MiB for full owned copies.

When `AuditLog` is attached to `FileOperationalJournal`, live entries and safe
restored entries retain the same immutable `Arc<OperationalJournalRecord>`.
Journal load first validates the hash chain, then uses an allocation-free text
check. Unsafe content is bounded, redacted and atomically rewritten once;
subsequent log attachment copies only bounded `Arc` handles. Public owned
accessors, snapshot serialization and V1 disk encoding are unchanged. A
one-attachment workload over 384 safe entries (about 3 MiB) reduced p50 from
12.26 ms to 86.50 us (-99.29%), peak RSS by 0.57% and workload RSS by 1.72% on
Apple M1. The paired real-fsync workload retains its earlier 27.83% p50,
37.30% peak-RSS and 47.93% retained-memory improvements.

The process-local `EventBus` has the same explicit memory shape: at most 10,000
events and a shared 16 MiB default retained-byte budget. `with_max_bytes`
tightens it, `stats` exposes current/peak bytes, evictions and rejections, and
`snapshot().recent(limit)` selects a stable newest page without cloning event
payloads. A 1,000-of-10,000 selection measured 2.39 us p50 and 8.48 MiB peak
RSS, versus 2.09 ms and 14.59 MiB for the compatible full-copy adapter.
With an attached `FileOperationalJournal`, both owners retain one immutable
`Arc<OperationalJournalRecord>` per event instead of two payload allocations.
Journal restore copies only bounded `Arc` handles. Public owned accessors,
snapshot serialization and the V1 on-disk record remain unchanged. A 3 MiB
retained-event workload reduced peak RSS from 8.11 to 5.08 MiB (-37.38%) and
retained workload memory by 48.00%; its disk-dominated p50 changed by +0.95%.

New applications consume application contracts through `appcore_sdk`; they do
not assemble the core manually. Keep I/O adapters and domain behavior outside
this crate.

**Maturity:** stable low-level RC surface; builder/plugin APIs are compatibility
level, while manifest-first hosting is preferred.
