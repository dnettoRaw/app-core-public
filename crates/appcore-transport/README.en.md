# appcore-transport

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Bounded HTTP and TLS client primitives shared by Runtime infrastructure.

The crate has independent SemVer and no AppCore dependencies. Infrastructure
adapters can consume it without the Runtime host.

Main contracts: targets, requests, responses, headers, reusable `HttpClient`,
per-exchange deadlines, bounded per-origin pooling, cancellation, transport
errors, response parsing and bounded gzip.

Own and clone one `HttpClient` to reuse fully drained HTTP/1.1 connections.
`HttpPoolConfig` bounds active connections, idle connections and retained
origins. `HttpTimeouts` separates connect/pool admission, read and write
deadlines. A truncated, malformed or `Connection: close` response is never
returned to the pool. The existing `send` function remains a one-shot V1
adapter and continues to send `Connection: close`.

Authentication and provider policy remain in the consuming crate. This is not a
general web framework.

`Debug` output reports request/response body sizes instead of body bytes.
Authorization, cookie and API-key header names are redacted even when a caller
did not explicitly mark the header as sensitive.

```bash
cargo test -p appcore-transport
```
