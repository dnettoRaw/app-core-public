# appcore-api

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Runtime HTTP command/query/status DTOs, router and host.

Stable routes include health, status, command and query V1 endpoints. Business
behavior is registered through command/query contracts; product REST resources
do not belong in this crate.

`HttpApiConfig::max_payload_bytes` bounds the complete command/query HTTP body
before JSON deserialization. Protected routes reject missing, malformed or
duplicate `Authorization` headers.

The host capability policy authorizes application commands and queries before
dispatch. Runtime-owned status queries remain outside application capability
declarations.

Runtime hosts freeze `ApiRouter` query registration after bootstrap. Router
clones share `Arc` endpoints, so direct facade, HTTP and peer RPC queries
release the host-state mutex before calling an endpoint and independent queries
can execute concurrently.

The 1.5 alpha opt-in `ReloadableRuntimeHttpHost` keeps one listener while it
health-checks and atomically switches routing generations. Requests already
admitted keep the old router until completion; the old generation drains under
a deadline. Prepare, post-switch health, or drain failure leaves or restores
the previous generation. Generation numbers increase monotonically, reloads
are serialized, and snapshots expose only bounded counters. A listener-address
change fails explicitly and requires a separately prepared listener generation
in the composition root. `RuntimeHttpHost` remains unchanged.

Composition roots that need bind-before-start validation can call
`run_on_listener_until_shutdown` with an already bound TCP listener. Ownership
is transferred to the host and shutdown remains graceful.

When composed with the `appcore-sync` 1.5 alpha candidate,
`SyncLogView::len` and `is_empty` are fallible. Private status JSON returns
`sync_log_len: null` together with
`sync_log_observation_ok: false` when live persistence cannot be observed; it
never substitutes a stale static count.

`HttpCommandAuth::default()` requires authentication and fails closed until a
token verifier is configured. Only `insecure_local_for_testing()` explicitly
disables command/query authentication for controlled local tests. `/v1/health`
remains intentionally public. Rejected command authorization is audited with
normalized metadata and never records credentials, payloads or idempotency
keys.

```bash
cargo test -p appcore-api
```
