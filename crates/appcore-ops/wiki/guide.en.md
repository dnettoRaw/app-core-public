# appcore-ops

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** vendor-neutral Runtime health, logging, metrics,
observations, heartbeat and availability.

**Internal dependencies:** `appcore-core`, `appcore-supervisor`.

**Primary API:** health status/report/checks, heartbeat sources, log levels and
logger implementations, metric counters and in-memory metrics,
`ObservationEvent`/`ObservationSink`, bounded file sink and statistics,
availability reports and compatibility reexports for
`appcore-supervisor::managed_services`.

The process-local observation sink retains at most 65,536 events and 16 MiB,
with a tighter byte ceiling derived from smaller count policies. The metric
registry retains at most 4,096 names, 128 bytes per name and 1 MiB aggregate.
Both expose count/byte pressure and immutable shared snapshots; compatibility
snapshots still produce owned values. Oversized observations are not retained
but continue to reach the at most 32 configured drains. The in-memory logger
likewise retains at most 4,096 records and 8 MiB and offers `shared_records`.
Drain configuration uses an immutable copy-on-write generation. Emission
shares that generation with one `Arc` clone instead of cloning as many as 32
drain handles, releases the configuration lock, and only then invokes drains.
`SharedObservationEvent::new` applies redaction and field bounds once. The
in-memory hub passes that immutable payload through
`ObservationSink::emit_shared`; the in-memory, file and metric sinks override
the method without deep cloning. Existing implementations only need `emit` and
use the compatible owned fallback automatically.
Sensitive attribute names use an allocation-free ASCII case-insensitive byte
scan. Conservative substring matching is unchanged, but no lowercase `String`
is created for each attribute.

The bounded file sink validates name, trace, attribute count, keys and values
again at `emit`, including events assembled through public fields. Its worker
measures one JSONL record with a limited counting writer before rotation and
then streams the same record to disk. It never allocates a complete serialized
copy. A record larger than the usable size of an empty file fails closed and is
reported by `FileObservationSinkStats::errors` without rotating the file. The
queue accepts at most 65,536 items and retains at most 8 MiB across queued and
currently written records. `FileObservationSink::pressure` exposes current and
peak bytes plus byte-budget rejections.

`FileObservationSink::flush` uses the 30-second
`FILE_OBSERVATION_FLUSH_TIMEOUT`. Its single deadline starts before bounded
queue admission and includes the worker's durability acknowledgement. Call
`flush_timeout(Duration::from_secs(...))` to use a smaller positive deadline.
A full queue or missing acknowledgement returns `ErrorKind::TimedOut`; a zero
or overflowing duration returns `InvalidInput`. If the command already entered
the queue, the worker may safely finish it after the caller times out.

Use it for generic operational signals. New service lifecycle code uses
`appcore-supervisor` directly. Do not add vendor SDK lock-in or application
business metrics to the Runtime crate.

**Maturity:** stable RC operational primitives; production export/collection is
deployment-owned.

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
