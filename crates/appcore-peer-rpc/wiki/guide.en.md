# appcore-peer-rpc

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** authenticated direct peer client, HTTP host, validation and
replay protection.

**Internal dependencies:** core, distributed contracts, security and transport.

**Primary API:** token issuer/authenticator/dispatcher traits and HashToken or
static implementations; in-memory/file nonce stores; validation config,
validator and signing/payload hashes; retry/client config and transport trait;
standard transport; HTTP state and host.

Use it only after tenant, cluster, source, target, protocol, expiry, nonce and
payload integrity can be established. `AllowPeerAuthenticator` is for tests,
not remote production.

Peer request, response, outbound and HTTP DTO `Debug` output reports payload
lengths and omits opaque bytes, credentials, nonce/idempotency values and remote
error details.

**Maturity:** stable peer protocol V1 RC surface.
