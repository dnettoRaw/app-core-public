# appcore-api

Command/query ingress admits at most 16 requests per host before collecting
or decoding bodies. Router clones share this gate; independently constructed
hosts do not. Saturation returns HTTP 503 without a waiting queue; body reception
has a 10-second deadline (408), and oversized bodies return 413. Each admitted
body is bounded by `max_payload_bytes`; raw body bytes are therefore bounded
by 16 times that setting, not total process RSS. Decoded objects, response
buffers and transport allocations are additional. Health/status bypass this
gate. The separate process-wide blocking-dispatch gate remains in place.

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Runtime HTTP command/query/status DTOs, router and host.

**Responsibility:** HTTP command/query/status host and transport DTOs.

**Internal dependencies:** `appcore-core`, `appcore-security` and
`appcore-supervisor`.

**Main API:** `CommandRequest`/`CommandResponse`,
`QueryRequest`/`QueryResponse`, validation errors, `CommandEndpoint`,
`QueryEndpoint`, `ApiRouter`, `ApiRequest`/`ApiResponse`, `RuntimeHttpHost`,
`HttpApiConfig`, static status, application command/query capability policy,
token verification and sync-log view.

Stable routes include health, status, command and query V1 endpoints. Business
behavior is registered through command/query contracts; product REST resources
do not belong in this crate.

`HttpApiConfig::max_payload_bytes` bounds the complete command/query HTTP body
before JSON deserialization. Protected routes reject missing, malformed or
duplicate `Authorization` headers.

The built-in TCP host closes a connection after 10 seconds without read
progress, including a client that stops during HTTP headers. Because no request
exists before the headers are complete, that case closes the socket instead of
returning an HTTP status. A formed request whose body stalls still receives
HTTP 408 from ingress middleware.

`QueryRequest::validate` measures its structured JSON through a bounded
counting writer instead of allocating a complete encoded copy. The exact V1
byte limit and `payload_bytes()` compatibility method remain unchanged, and
HTTP validates once before blocking dispatch.

The router owns one shared immutable `RuntimeStaticInfo`; cloning request state
does not copy its peer lists, DNS seeds, paths or identity strings. Blocking
dispatch takes ownership of command/query requests. Query audit keeps only the
bounded query ID and name while the payload is in flight.
Owned command paths use `CommandRequest::into_envelope`, which validates the
same V1 fields and transfers the payload allocation into `CommandEnvelope`
without copying its bytes. `to_envelope` remains for borrowed callers.

`CommandTokenVerifier` also has additive borrowed request methods. Their
defaults materialize `RequestValidationDetails` and call the existing owned
methods, so existing verifiers keep their behavior. The Runtime verifier
overrides them to hash text or structured JSON directly without an owned
payload copy.

The host capability policy authorizes application commands and queries before
dispatch. Runtime-owned status queries remain outside application capability
declarations.

Command and query dispatch share 16 process-wide blocking slots. The Tokio
blocking pool uses the same ceiling, 1 MiB stacks and a five-second idle
retirement. Saturation is rejected with HTTP 503 before work enters the queue.

Runtime hosts freeze `ApiRouter` query registration after bootstrap. Router
clones share `Arc` endpoints, so direct facade, HTTP and peer RPC queries
release the host-state mutex before calling an endpoint and independent queries
can execute concurrently.
`query_names_iter` exposes the frozen registry by reference for validation;
`query_names` remains the deterministic owned API for output boundaries. The
composition root uses the borrowed view, so a successful manifest check does
not clone or sort the complete query catalog.

The `1.0.2-rc` opt-in `ReloadableRuntimeHttpHost` keeps one listener while it
health-checks and atomically switches routing generations. Requests already
admitted keep the old router until completion; the old generation drains under
a deadline. Prepare, post-switch health, or drain failure leaves or restores
the previous generation. Generation numbers increase monotonically, reloads
are serialized, and the owner retains at most one active and one retiring
generation. A failed generation blocks another reload until its final request
releases it. Payload-free snapshots expose active/retiring admission and
in-flight counts without retaining a history. Cancelling after the switch
restores the previous generation synchronously. A listener-address change
fails explicitly and requires a separately prepared listener generation in
the composition root. `RuntimeHttpHost` remains unchanged.

Composition roots that need bind-before-start validation can call
`run_on_listener_until_shutdown` with an already bound TCP listener. Ownership
is transferred to the host and shutdown remains graceful.

When composed with `appcore-sync 1.0.2-rc`,
`SyncLogView::len` and `is_empty` are fallible. Private status JSON returns
`sync_log_len: null` together with
`sync_log_observation_ok: false` when live persistence cannot be observed; it
never substitutes a stale static count.

The built-in `runtime.audit` query caps `limit` at 1,000. It captures shared
record and entry snapshots while holding only short locks, then materializes
the newest requested page after those locks are released; it never deep-clones
the complete 10,000-item audit queues for a bounded response.

`runtime.events` follows the same rule: it borrows at most the newest 1,000
events from a shared snapshot after releasing the host and event-bus locks. The
response format remains unchanged and continues to omit opaque event payloads.

`HttpCommandAuth::default()` requires authentication and fails closed until a
token verifier is configured; `HttpCommandAuth::required` installs one
explicitly. `insecure_local_for_testing()` exists only for crate tests or debug
builds with `insecure-testing`, and built-in hosts reject that policy on a
non-loopback listener. Reload cannot change the authentication boundary.
`/v1/health` remains intentionally public but returns only `status`; Supervisor
details remain authenticated. Rejected command authorization is audited with
normalized metadata and never records credentials, payloads or idempotency
keys. Inbound TLS remains a deployment boundary.

**Maturity:** strict and stable RC HTTP V1 surface.

```bash
cargo test -p appcore-api
```

## Stable documentation

Stable ID: **ACR-009**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-009). This permanent ID
remains valid if the wiki page moves.
