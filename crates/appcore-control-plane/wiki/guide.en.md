# appcore-control-plane

Release candidate **`appcore-control-plane 1.0.4-rc.4`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.


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

## Release candidate — 2026-10-14

`1.0.4-rc.2`: dependency metadata alignment.


## Source fix: fencing-safe restore

`FileControlPlane::restore_from` now requires the current state to decode and the staged backup to contain exactly the same service-lease slots (including epoch tombstones, holder and renewal/expiry timestamps). Any difference returns `ControlPlaneError::Conflict` before replacement. Current-state corruption returns a controlled error, retaining current bytes. Source copying, validation and replacement stay under the provider's exclusive file lock; a rejected staged file is removed. Presence can still be restored when fencing is identical.

Migration: V1 JSON and Rust signatures are unchanged, but recovery is intentionally stricter. Restoring an older snapshot after acquisition, renewal, expiry/reacquisition or release is no longer permitted; corrupted current state cannot be repaired through this API because its fencing high-water marks cannot be proven. Keep the state and backup for review. Never delete/reinitialize the directory or downgrade to the old permissive behavior to bypass rejection. Total arbiter-state loss or filesystem rollback needs a separate deployment recovery/domain procedure and is not solved here. This is a local unpublished fix; consumers need a new immutable package release before adoption. It does not add Gateway authentication, distributed promotion or protection from restoring files outside this API.

Qualification: 45 focused crate tests passed on macOS, including the previously failing epoch rollback reproduction, released tombstones, same-epoch renewal/holder changes, unchanged-fence presence restore, corruption preservation, restart and racing independent provider handles. These tests do not qualify two physical nodes, DNET or Windows.

Restore temporarily retains current and staged decoded states: each is admitted to at most 262,144 records and 64 MiB estimated retained payload (128 MiB combined), in addition to bounded streaming buffers and allocator overhead. No complete encoded JSON copy is retained.


## Exact service authority — unpublished source

`ServiceLeaseAuthority` is a new additive contract in `appcore-distributed-contracts::control_plane::lease_authority`. Acquisition, exact renewal, exact release, inspection and check use the server clock. `FileControlPlane` implements it: TTL 1–30000 ms, existing same-Core acquisition returns the live grant unchanged, renewal cannot reacquire expiry, and changed grants are rejected. Data writer and update coordinator use separate deployment-selected `ServiceId`s. Heartbeat, role flags and Gateway HA instance epochs confer no data-write authority.

On restart use `FileControlPlane::open_existing_with_clock`; it rejects missing/corrupt state and never initializes a new epoch domain. Initial provisioning via `with_clock` is an explicit deployment operation. Authority operations use a nonwaiting OS lock and report contention/unavailability rather than queue indefinitely. `with_fenced_service_write` checks the exact lease and runs a trusted bounded local commit under that same lock; takeover cannot overlap that commit even if expiry occurs during it. The callback must not reenter the authority and the host owns its I/O bounds and application idempotency. A remote check response is only an observation: a remote store must atomically enforce fencing at commit. Do not cache a successful check or treat transport loss as permission.

Gateway adds `make_service_lease_router(gateway, authority, replay)` as explicit composition, absent from the default router. POST `/v1/gateway/service-lease` accepts `ServiceLeaseRequestV1` (version=1, identity, service_id, action); actions are acquire/renew/release/inspect/check. Authentication is mandatory even in insecure-testing. The isolated `service_lease_token_claims` domain requires exact command/query name, Core subject, full request hash and a single-use JTI of at most 96 bytes. TTL of a credential cannot exceed the requested acquisition lifetime or the grant expiry for grant-bound actions. Relay tokens, altered scope/action/epoch or wildcard credentials do not authorize it. The issuer must independently authorize tenant/cluster/Core/service/action before signing; clients must not receive the signing secret.

Compose a dedicated durable `PeerNonceStore` shared across restarts and authority replicas, for example `FilePeerNonceStore` on certified storage. No default is inferred. Bodies are limited to 16 KiB and only eight operations are admitted, without an admission queue. Authentication/replay I/O and authority calls run on blocking workers outside the async polling thread. Deployment owns provider I/O deadlines, graceful draining, TLS, shared clock policy and state custody. Response codes: 401 missing credentials, 403 invalid/replayed credentials, 409 stale/conflicting grant, 503 unavailable/contended authority or saturation. A rejected request never becomes an offline success. Provider failure or a lost response is ambiguous: obtain fresh authorized inspection, never guess success or rerun a write under a new epoch automatically.

Limits: a Core is the writer identity, not a PID; sharing Core credentials shares authority. Provision distinct Core identities for concurrent writers and retain a stable logical service for discovery. This reference is not consensus, not an HA arbiter with independent disks and not a remote storage fencing implementation. Total loss/rollback of authority or replay history requires controlled recovery/new authority domain; deleting files or calling the initializing constructor is forbidden as a recovery shortcut. V1 wire/storage and existing APIs remain unchanged. Consumer wiring, registry publication and real DNET/two-device partition qualification remain pending.
