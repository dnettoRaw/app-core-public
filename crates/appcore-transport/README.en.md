# appcore-transport

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Bounded HTTP and TLS client primitives shared by Runtime infrastructure.

The crate has independent SemVer and no AppCore dependencies. Infrastructure
adapters can consume it without the Runtime host.

Main contracts: targets, requests, responses, headers, client configuration,
cancellation, transport errors, response parsing and bounded gzip.

Authentication and provider policy remain in the consuming crate. This is not a
general web framework.

`Debug` output reports request/response body sizes instead of body bytes.
Authorization, cookie and API-key header names are redacted even when a caller
did not explicitly mark the header as sensitive.

```bash
cargo test -p appcore-transport
```
