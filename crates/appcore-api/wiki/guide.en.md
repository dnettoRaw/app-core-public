# appcore-api

Next-major sync-log observations are fallible. Private status and diagnostics
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
Do not add product REST resources or business schemas. New application hosting
normally reaches it through `appcore-bin`.

Application queries are authorized by the composed capability policy before
the application router runs. Runtime-owned status queries remain outside the
application capability catalog.

Runtime hosts freeze `ApiRouter` query registration after bootstrap. Router
clones share `Arc` endpoints, so direct facade, HTTP and peer RPC paths release
the host-state mutex before endpoint execution. Independent queries can run
concurrently; a late `register_query` call fails with `router_frozen`.

The configured payload bound applies to the complete HTTP body before Axum
deserializes JSON. Protected routes accept exactly one well-formed bearer
`Authorization` header; duplicates fail closed.

`HttpCommandAuth::default()` requires authentication and fails closed until a
token verifier is configured. Only `insecure_local_for_testing()` explicitly
disables command/query authentication for controlled local tests. `/v1/health`
remains intentionally public. Rejected command authorization is audited with
normalized metadata and never records credentials, payloads or idempotency
keys.

**Maturity:** stable strict HTTP V1 RC surface.
