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

**Maturity:** stable V1 wire contract; serialized compatibility is strict.
