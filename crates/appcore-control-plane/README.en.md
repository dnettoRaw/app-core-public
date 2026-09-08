# appcore-control-plane

Dropping a request future before worker dispatch prevents its operation from
running. The queue holds a weak result owner, so it does not retain an abandoned
future's waker or response. The queued closure and its slot remain until dequeued.
Once dispatch begins, dropping the future cannot undo remote effects or interrupt
a synchronous transport; use cooperative cancellation and reconcile ambiguity.

Lease acquisition/renewal and release use one HTTP attempt, regardless of
retry configuration. V1 supplies no remote deduplication key; a lost response
may follow an applied mutation. Timeout or transient HTTP failure must not be
treated as proof that the lease was unchanged. Reconcile authoritative lease
state and fencing before leader-dependent writes; do not blindly replay the
operation. Discovery, registration and heartbeat retain their configured retry
policy; their remote semantics still require deployment conformance testing.

Retry waits use non-cryptographic equal jitter between half (rounded up) and
the current exponential backoff ceiling, then clamp to remaining cycle time.
A zero backoff stays zero. Each cycle seeds local scheduling state from the
standard library's randomized hasher; this is not security randomness.

HTTP retry configuration is bounded at request execution: at most 16 attempts,
1–30,000 ms per attempt and at most 30,000 ms per backoff. Zero attempts still
means one attempt; zero backoff remains supported. The conservative cycle budget
is attempts × timeout + (attempts − 1) × maximum backoff, capped at 120 seconds.
A monotonic clock clamps each attempt and retry wait to remaining time. An
overdue transport response returns Timeout without another attempt, even if it
reports success; the remote operation may already have applied. External
transports must honor deadlines themselves: synchronous callbacks cannot be
forcibly interrupted. The budget starts at the provider call, including queue
wait, request serialization and response decoding. Expired queued work does
not reach the transport. Expiry is observed when the worker proceeds, not by
an independent timer: a stuck earlier transport can delay future completion.

HTTP retries are limited to 408, 429, 500, 502, 503 and 504 responses,
and transport, timeout or offline failures. Other status codes and typed
semantic failures return immediately. The first exponential-backoff delay is
also capped by `max_backoff_ms`. The total local deadline described above does
not prove that an ambiguous remote write is safe to repeat.

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Presence, heartbeat, discovery and lease implementations for distributed
Runtime operation.

**Responsibility:** generic presence, heartbeat, discovery and lease
implementations.

**Internal dependencies:** contracts, core, distributed contracts and
transport.

**Main API:** in-memory, file and offline clients; HTTP configuration, retry
policy and transport trait; standard one-shot, pooled and bearer transports;
coordinator and heartbeat policy; global/service leadership guards; secure
endpoint validation.

Available profiles include in-memory, offline, file and bounded HTTP clients.
The control plane coordinates Runtime infrastructure and never stores business
payloads. File operation requires certified locks/storage. Remote operation
requires deployment-owned TLS and authentication.

`PooledHttpTransport` is the reusable unauthenticated HTTP profile;
`BearerHttpTransport` also reuses its bounded client. `StdHttpTransport`
remains the one-shot V1 compatibility profile.
`HttpControlPlaneClient` converts each encoded body once into a
`SharedHttpControlPlaneRequest` and borrows it across bounded retries. Built-in
transports reuse the same immutable body allocation; external transports that
only implement the original owned method keep the compatible fallback. Request
`Debug` output reports body length without exposing its bytes.

The file profile bounds persisted state and backups to 16 MiB. Lease expiry and
epoch arithmetic fail closed on overflow; an exhausted epoch is never reused as
a fencing token.

The in-memory profile admits at most 65,536 combined registrations and service
lease slots under a 16 MiB estimated retained-byte budget by default. Use
`with_limits` to tighten both limits and `stats` to observe current/peak bytes
and rejections. An oversized replacement leaves its previous registration or
lease unchanged. The file profile additionally caps decoded state at 262,144
records and 64 MiB while retaining its unchanged 16 MiB V1 JSON limit.

State JSON is decoded through a bounded reader and serialized directly into an
exclusive temporary file. Backup and restore stream one bounded file into a
staged generation, validate that exact generation, sync it, and only then
replace the destination. The decoded maps remain the sole full state owner; no
second full encoded JSON buffer is retained.

**Maturity:** stable RC contracts and references; operation of an external
service belongs to the deployment.

```bash
cargo test -p appcore-control-plane
```

## Stable documentation

Stable ID: **ACR-015**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-015). This permanent ID
remains valid if the wiki page moves.
