# appcore-ops

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Vendor-neutral health, heartbeat, logging, metrics, observations and
availability. Managed-service compatibility APIs are reexported from
`appcore-supervisor`; new lifecycle code should depend on that crate directly.

**Responsibility:** vendor-neutral health, logs, metrics, observations,
heartbeat and availability.

**Internal dependencies:** `appcore-core`, `appcore-supervisor`.

**Main API:** health status/report/checks, heartbeat sources, loggers, metric
counters, `ObservationEvent`/`ObservationSink`, bounded file sink, availability
report and compatibility reexports for `appcore-supervisor::managed_services`.

Signals, queues and files are bounded. Provider-specific exporters and
application business metrics remain outside this Runtime crate.

`InMemoryObservationSink` now caps both entries and estimated retained bytes,
never preallocates from an untrusted capacity, and offers immutable
`ObservationSnapshot` views. `InMemoryMetrics` likewise caps metric-name length,
cardinality and bytes; rejected names remain visible through pressure counters.
The compatibility `snapshot` methods still return owned values, while
`shared_snapshot` avoids cloning retained names and events.
`InMemoryLogger` applies the same pattern to 4,096 records and 8 MiB; its
`shared_records` view avoids cloning redacted log text.
Drain configuration is an immutable copy-on-write generation. Each observation
shares that generation with one `Arc` clone instead of cloning up to 32 drain
handles, and every callback still runs after the configuration lock is released.
`SharedObservationEvent::new` redacts and bounds an event once. The in-memory,
file and metric sinks override `ObservationSink::emit_shared` so one immutable
payload can be retained or inspected by every drain; the default implementation
preserves existing owned-only sink implementations.
Sensitive attribute keys are screened with an allocation-free ASCII
case-insensitive byte scan. The existing conservative substring policy remains
unchanged, without allocating a lowercase `String` for every attribute.

The file observation worker revalidates public event fields before bounded
queue admission. It counts each JSONL record through a limited writer and then
serializes directly to the active file, without retaining a second complete
JSON buffer. A record that cannot fit beside the V1 header in an empty file is
rejected and increments `FileObservationSinkStats::errors`; it never creates an
oversized rotation. Admission also caps the queue at 65,536 items and 8 MiB
across queued and currently written records. `FileObservationSink::pressure`
reports current bytes, peak bytes and byte-budget rejections.
`flush` applies the 30-second `FILE_OBSERVATION_FLUSH_TIMEOUT` to both bounded
queue admission and the worker acknowledgement. Use `flush_timeout` for a
smaller positive operational deadline; saturation or a stalled worker returns
`TimedOut`. A flush already admitted to the worker can finish after the caller
times out, without duplicating or force-cancelling filesystem I/O.

```bash
cargo test -p appcore-ops
```

## Metric snapshot retention

A shared metric snapshot is immutable, but keeping it across an update makes
that update copy the map nodes. Names remain shared. Registry pressure covers
the current generation only, not all snapshots held by consumers. A bound on
names is therefore not a bound on total process memory.

A collector should drop its prior snapshot before acquiring the next, or use
an explicit bounded queue that evicts before admission. Bound all consumers
and their clones, including in-flight exports. Do not replace snapshot values
with live atomic reads: that would change point-in-time semantics.

The `metric_update_4096_retained_0/1/16` benchmark family interleaves snapshots
and updates of 4,096 counters with 0, 1 or 16 retained generations. Fixture
creation and retention warmup are outside timing; snapshot acquisition,
eviction and update are timed. RSS includes warmed generations; the retained
checkpoint follows their release and can include allocator retention. These
cases do not certify a deployment's aggregate memory budget.

**Maturity:** stable RC primitives; production export and collection belong to
the deployment.

## Stable documentation

Stable ID: **ACR-013**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-013). This permanent ID
remains valid if the wiki page moves.
