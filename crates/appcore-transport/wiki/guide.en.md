# appcore-transport

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** shared bounded HTTP and TLS client mechanics.

**Internal dependencies:** none.

**Versioning:** independent SemVer. The crate can be consumed without any
other AppCore package.

**Primary API:** `HttpScheme`, `HttpTarget`, `HttpRequest`, `HttpHeader`,
`HttpClientConfig`, `HttpResponse`, `CancellationToken`, `TransportError`,
`send`, response parsing and bounded gzip encode/decode.

Use it inside infrastructure adapters that need the same size, timeout,
cancellation and TLS mechanics. Consumers still own authentication and policy.
Do not turn it into a general web framework or add business endpoints.

Request/response `Debug` output contains body lengths, not body bytes. Known
credential headers are redacted even if a caller used the non-sensitive header
constructor.

**Maturity:** stable infrastructure RC surface.
