# appcore-transport

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Bounded HTTP and TLS client primitives shared by Runtime infrastructure.

**Responsibility:** shared, bounded HTTP/TLS client mechanics.

**Internal dependencies:** none.

The crate has independent SemVer and no AppCore dependencies. Infrastructure
adapters can consume it without the Runtime host.

**Main API:** targets, requests, responses, headers, reusable `HttpClient`,
per-exchange deadlines, bounded per-origin pooling, cancellation, transport
errors, response parsing and bounded gzip.

Own and clone one `HttpClient` to reuse fully drained HTTP/1.1 connections.
`HttpPoolConfig` bounds active connections, idle connections and retained
origins. `HttpTimeouts` separates connect/pool admission, read and write
deadlines. A truncated, malformed or `Connection: close` response is never
returned to the pool. The existing `send` function remains a one-shot V1
adapter and continues to send `Connection: close`.

Request bodies are immutable shared bytes. `HttpRequest::new` preserves its
owned `Vec` input contract and moves it into shared storage;
`HttpRequest::from_shared_body` accepts an existing `Arc<[u8]>`. Cloning a
request or transferring it to a bounded worker therefore does not duplicate a
large body.

Authentication and provider policy remain in the consuming crate. This is not a
general web framework.

`Debug` output reports request/response body sizes instead of body bytes.
Authorization, cookie and API-key header names are redacted even when a caller
did not explicitly mark the header as sensitive.

```bash
cargo test -p appcore-transport
```

`encode_gzip_if_smaller` stops retaining a gzip candidate when emitted bytes
would reach the input length and returns `None`. Output growth requests never
exceed `input.len() - 1`; empty input returns `None` without creating a codec.
This does not bound codec workspace, allocator overhead, input memory or CPU
already spent before the codec emits bytes. Useful candidates retain the same
gzip bytes; no compression-ratio heuristic skips potentially useful input.

For non-chunked gzip responses, parsing borrows compressed bytes directly
from the input while decoding, avoiding a second compressed-body allocation.
The returned body remains owned. Chunked gzip is compacted in place first but
still needs a decompressed output; this is not streaming.

`parse_response_owned` accepts ownership of a complete frame. It compacts both
fixed and chunked identity bodies inside that same allocation and returns it
without allocating a second body. `HttpClient` and one-shot `send` use this
path. Compressed responses retain their bounded decode output; the borrowed
`parse_response` API remains unchanged.

**Maturity:** stable RC infrastructure surface.

## Stable documentation

Stable ID: **ACR-004**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-004). This permanent ID
remains valid if the wiki page moves.
