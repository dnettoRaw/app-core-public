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

Use it inside infrastructure adapters that need the same size, timeout,
cancellation and TLS mechanics. Consumers still own authentication and policy.
Do not turn it into a general web framework or add business endpoints.

Request/response `Debug` output contains body lengths, not body bytes. Known
credential headers are redacted even if a caller used the non-sensitive header
constructor.

**Maturity:** stable infrastructure RC surface.
