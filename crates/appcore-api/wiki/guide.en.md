# appcore-api

Command/query ingress admits at most 16 requests per host before collecting
or decoding bodies. Router clones share this gate; independently constructed
hosts do not. Saturation returns HTTP 503 without a waiting queue; body reception
has a 10-second deadline (408), and oversized bodies return 413. Each admitted
body is bounded by `max_payload_bytes`; raw body bytes are therefore bounded
by 16 times that setting, not total process RSS. Decoded objects, response
buffers and transport allocations are additional. Health/status bypass this
gate. The separate process-wide blocking-dispatch gate remains in place.

The `appcore-sync 1.0.2-rc` observations are fallible. Private status and diagnostics
expose `sync_log_len: null` plus `sync_log_observation_ok: false` when the live
provider cannot be read, rather than reporting stale state.

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** Runtime HTTP command/query/status host and transport DTOs.

**Internal dependencies:** `appcore-core`, `appcore-security` and
`appcore-supervisor`.

**Primary API:** `CommandRequest`/`CommandResponse`,
`QueryRequest`/`QueryResponse`, validation errors, `CommandEndpoint`,
`QueryEndpoint`, `ApiRouter`, generic `ApiRequest`/`ApiResponse`,
`RuntimeHttpHost`, `HttpApiConfig`, static status information, application
command/query capability policy, token verification and sync-log view.

Use it to expose Runtime-owned routes and register application query behavior.
Do not add product REST resources or business schemas. New applications reach
application contracts through `appcore-sdk`; HTTP composition is explicit.

Application queries are authorized by the composed capability policy before
the application router runs. Runtime-owned status queries remain outside the
application capability catalog.

Runtime hosts freeze `ApiRouter` query registration after bootstrap. Router
clones share `Arc` endpoints, so direct facade, HTTP and peer RPC paths release
the host-state mutex before endpoint execution. Independent queries can run
concurrently; a late `register_query` call fails with `router_frozen`.
`query_names_iter` borrows the frozen registry for internal validation, while
`query_names` keeps deterministic owned ordering for output boundaries. The
manifest check therefore scans names without cloning the complete catalog.

In `1.0.2-rc`, `ReloadableRuntimeHttpHost` provides an explicit routing
generation transaction. `prepare` accepts only a newer generation on the same
bound address. `reload` runs `/v1/health` before activation, atomically changes
new-request routing, checks health again, and drains the old in-flight count.
If switch health or drain fails, the old generation is restored and the failed
one stops admission before cleanup. An accepted request never changes router.
Timeouts are non-zero and capped at 60 seconds; snapshots contain generation,
in-flight, success, failure, and rollback counters without request identities.
The owner keeps at most one active and one retiring generation. A failed
generation that still has requests blocks the next reload, and its final permit
releases the Router without a cleanup task. `generation_snapshot` exposes this
bounded active/retiring state without payloads or history. Cancelling after the
switch synchronously restores the previous generation before admission reopens.

Address changes are intentionally outside this stable-listener primitive. The
composition root must prepare a second listener and coordinate it through the
existing Supervisor. There is no automatic V1 manifest watcher or fallback.
For bind-before-start validation on the stable address, the composition root
may transfer an already bound TCP listener through
`run_on_listener_until_shutdown`.

The configured payload bound applies to the complete HTTP body before Axum
deserializes JSON. Protected routes accept exactly one well-formed bearer
`Authorization` header; duplicates fail closed.

The built-in TCP host closes a connection after 10 seconds without read
progress, including incomplete HTTP headers. No HTTP response can be formed
before a request exists, so header inactivity closes the socket. Body
inactivity after a complete request still returns HTTP 408.

Structured query validation streams JSON into a bounded counting writer. It
therefore enforces the exact serialized-byte limit without retaining an encoded
`Vec<u8>`, while the public `payload_bytes()` method remains compatible. The
HTTP path validates once before the request crosses into blocking dispatch.

The router owns one shared immutable `RuntimeStaticInfo`; cloning request state
does not copy its peer lists, DNS seeds, paths or identity strings. Blocking
dispatch takes ownership of command/query requests. Query audit keeps only the
bounded query ID and name while the payload is in flight.
Use `CommandRequest::into_envelope` when the caller owns the request: it keeps
the V1 validation and moves the existing UTF-8 allocation into the core byte
payload. The compatible `to_envelope` method serves borrowed requests.

`CommandTokenVerifier` also has additive borrowed request methods. Their
defaults materialize `RequestValidationDetails` and call the existing owned
methods, so existing verifiers keep their behavior. The Runtime verifier
overrides them to hash text or structured JSON directly without an owned
payload copy.

Command and query dispatch share 16 process-wide blocking permits. The runtime
uses at most 16 blocking threads with 1 MiB stacks and retires idle threads
after five seconds. A full gate returns HTTP 503 before queue admission.

The built-in `runtime.audit` query caps `limit` at 1,000. It takes shared
record and entry snapshots under short locks and materializes only the newest
requested page after releasing them. It does not deep-clone either complete
10,000-item queue. The shared 1,000-of-10,000 selection measured 2.06 us p50
and 11.88 MiB peak RSS, versus 4.16 ms and 20.33 MiB for full owned copies.

`runtime.events` uses the same snapshot boundary and still caps the newest page
at 1,000 while omitting opaque payloads from its unchanged response. Selecting
1,000 of 10,000 events measured 2.39 us p50 and 8.48 MiB peak RSS, versus
2.09 ms and 14.59 MiB for cloning the complete history.

`HttpCommandAuth::default()` requires authentication and fails closed until a
token verifier is configured. Only `insecure_local_for_testing()` explicitly
disables command/query authentication for controlled local tests. `/v1/health`
remains intentionally public. Rejected command authorization is audited with
normalized metadata and never records credentials, payloads or idempotency
keys.

**Maturity:** stable strict HTTP V1 RC surface.
