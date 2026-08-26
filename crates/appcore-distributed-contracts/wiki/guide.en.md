# appcore-distributed-contracts

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** versioned control-plane and peer RPC wire/provider
contracts.

**Internal dependencies:** `appcore-contracts`, `appcore-types`.

**Primary API:** control-plane protocol constants and paths, registration,
presence, heartbeat, peer directory, global compatibility leases,
service-scoped leases, leadership decisions and provider traits; peer protocol
paths, envelopes, responses, errors, call kinds, advertisement DTOs, client
executor trait and opaque content-envelope transport metadata.

Implementations belong in control-plane or peer crates. Do not add HTTP clients,
filesystem state, tokens or product capability rules here.

Opaque-content and Peer RPC wire serialization is unchanged. Their `Debug`
implementations expose lengths and routing metadata instead of opaque payload
bytes, nonce/idempotency values or remote error details.

`peer_rpc::v2` is an independent opt-in frame family. An open frame fixes the
aggregate quota, chunk size/count and deadline; chunk frames carry exact
sequence, encoding, decoded length and per-chunk digest; commit binds the total
decoded length and digest; cancel has a controlled reason. Encoded bytes use a
canonical base64 JSON string rather than an integer array. V1 and V2 use
separate modules and routes. No parser detects, upgrades or falls back between
them.

**Maturity:** stable V1 wire contract; post-1.0 V2 chunk contract in development.
