# appcore-transport

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** shared bounded HTTP and TLS client mechanics.

**Internal dependencies:** none.

**Versioning:** independent SemVer. The crate can be consumed without any
other AppCore package.

**Primary API:** `HttpScheme`, `HttpTarget`, `HttpRequest`, `HttpHeader`,
`HttpClient`, `HttpExchangeConfig`, `HttpTimeouts`, `HttpPoolConfig`,
`HttpClientConfig`, `HttpResponse`, `CancellationToken`, `TransportError`,
`send`, response parsing and bounded gzip encode/decode.

One `HttpClient` owns a bounded pool keyed by scheme, host and port. Its clones
share that pool. Admission is capped per origin, waiting is bounded by the
connect deadline and cancellable, retained origins and idle sockets are capped,
and idle sockets expire. Only a completely framed and parsed response can make
its socket reusable. Truncation, malformed framing, timeout, cancellation,
`Connection: close` and close-delimited bodies discard the socket.

Use `HttpExchangeConfig` and `HttpTimeouts` when connect/pool admission, read
and write need independent deadlines. `HttpClientConfig` and the free `send`
function retain the V1 one-shot contract, including `Connection: close`; they
do not opt an existing consumer into pooling silently.

`HttpRequest` stores its body as immutable shared bytes. The compatible `new`
constructor moves an owned `Vec<u8>` into that storage, while
`from_shared_body` reuses a caller-owned `Arc<[u8]>`. Request clones share the
same allocation; this is useful when a bounded transport worker must own the
request after the calling future yields.

Use it inside infrastructure adapters that need the same size, timeout,
cancellation and TLS mechanics. Consumers still own authentication and policy.
Do not turn it into a general web framework or add business endpoints.

Request/response `Debug` output contains body lengths, not body bytes. Known
credential headers are redacted even if a caller used the non-sensitive header
constructor.

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

When the complete frame is already owned, `parse_response_owned` reuses that
allocation for fixed and chunked identity bodies. The built-in clients use it;
borrowed callers keep `parse_response`. Compressed decode remains bounded but
still requires owned output.

**Maturity:** stable infrastructure RC surface.
