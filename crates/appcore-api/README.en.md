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

On the next-major development line, `SyncLogView::len` and `is_empty` are
fallible. Private status JSON returns `sync_log_len: null` together with
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
