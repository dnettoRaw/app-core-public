# appcore-control-plane

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** generic presence, heartbeat, discovery and lease
implementations.

**Internal dependencies:** contracts, core, distributed contracts and
transport.

**Primary API:** in-memory, file and offline control-plane clients; HTTP request
configuration, retry policy and transport trait; standard/bearer HTTP
transports; coordinator and heartbeat policy; static global/service leadership
guards; secure endpoint validation.

Use it to implement distributed coordination without business payloads.
File-backed profiles require certified locking/storage semantics. Remote
profiles require deployment TLS and authentication.

The file profile caps state and backup input at 16 MiB and rejects malformed or
future state. Expiry and epoch arithmetic is checked; epoch exhaustion fails
closed instead of reusing a fencing token.

**Maturity:** stable RC contracts and reference implementations; external
service operation is deployment-owned.
