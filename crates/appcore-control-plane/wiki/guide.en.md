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
also capped by `max_backoff_ms`. This policy does not establish a total
operation deadline or prove that an ambiguous write is safe to repeat.

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** generic presence, heartbeat, discovery and lease
implementations.

**Internal dependencies:** contracts, core, distributed contracts and
transport.

**Primary API:** in-memory, file and offline control-plane clients; HTTP request
configuration, retry policy and transport trait; standard one-shot,
pooled/bearer HTTP transports; coordinator and heartbeat policy; static
global/service leadership guards; secure endpoint validation.

Use `PooledHttpTransport` for reusable unauthenticated calls.
`BearerHttpTransport` also owns a reusable bounded client. Keep
`StdHttpTransport` only where the V1 one-shot `Connection: close` behavior is
required.
`HttpControlPlaneClient` converts an encoded body once into a
`SharedHttpControlPlaneRequest` and borrows it for every bounded retry. The
built-in transports clone only its shared owner; existing external transports
implementing the owned method continue through the compatible default. Both
request owners omit body bytes from `Debug` output.

Use it to implement distributed coordination without business payloads.
File-backed profiles require certified locking/storage semantics. Remote
profiles require deployment TLS and authentication.

The file profile caps state and backup input at 16 MiB and rejects malformed or
future state. Expiry and epoch arithmetic is checked; epoch exhaustion fails
closed instead of reusing a fencing token.

`InMemoryControlPlane` defaults to 65,536 combined registrations/lease slots
and a 16 MiB estimated retained-byte budget. `with_limits` can tighten both;
`stats` exposes current/peak bytes, record counts and rejected admissions.
Rejection is atomic: an existing record remains usable. File state keeps the
16 MiB V1 JSON boundary and additionally limits decoded state to 262,144
records and 64 MiB.

Its V1 JSON is decoded through a bounded reader and written directly to an
exclusive temporary file. Backup and restore copy one bounded stream, validate
the exact staged generation, sync it, and only then replace the destination.
Only the decoded maps hold the complete state in memory; persistence does not
retain another full encoded JSON buffer.

**Maturity:** stable RC contracts and reference implementations; external
service operation is deployment-owned.
