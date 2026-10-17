# appcore-distributed-contracts

Release candidate **`appcore-distributed-contracts 1.0.2-rc.1`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.


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

`OpaqueEnvelopeDeduplicator` stores one shared allocation for each accepted
message ID across the membership set and acceptance-order queue. The public
API, duplicate outcome and bounded FIFO eviction are unchanged. With 65,536
distinct 128-byte IDs on Apple M1, p50 fell from 37.55 ms to 32.83 ms and peak
RSS from 35.86 MiB to 27.25 MiB. Both `validate_transport` and `accept` reject
empty IDs, control characters and IDs above `MAX_OPAQUE_MESSAGE_ID_BYTES`
(1,024 UTF-8 bytes). Rejection happens before allocation or FIFO eviction.

`peer_rpc::v2` is an independent opt-in frame family. An open frame fixes the
aggregate quota, chunk size/count and deadline; chunk frames carry exact
sequence, encoding, decoded length and per-chunk digest; commit binds the total
decoded length and digest; cancel has a controlled reason. Encoded bytes use a
canonical base64 JSON string rather than an integer array. V1 and V2 use
separate modules and routes. Human-readable encoding emits base64 through a
fixed 3 KiB input/4 KiB output scratch buffer. Decoding borrows the encoded
JSON string where the deserializer supports it, then allocates only the
required decoded bytes. Exact fixtures do not change. No parser detects,
upgrades or falls back between versions.

The optional V2 binary codec is a separate, explicitly selected representation.
Its fixed `APCRPC2B` marker, codec version, frame/reply kind and exact length
bind one bounded Postcard payload. Non-human serializers carry chunk payloads
as native byte strings; the existing human-readable JSON representation stays
canonical base64. Encoding and decoding accept a caller limit and always apply
the protocol ceiling of 256 KiB. A message kind, marker, version, length or
codec mismatch fails before the frame reaches an implementation.

`PeerRpcWireErrorV2` carries fixed `code`, `phase` and `retryable` metadata,
optional bounded `retry_after_ms` and `correlation_id`, and an exact redacted
message. Decoding validates the whole matrix. Contradictory known metadata is
invalid; an unknown code discards its message/hint and becomes terminal
`unknown`. `PeerRpcRemoteErrorV1` separately decodes only exact stable V1
strings, so free-form remote text cannot select retry behavior.

**Maturity:** stable V1 wire contract; post-1.0 V2 chunk contract in development.

## Operator diagnostic projection

This section describes unpublished source changes. Verify the exact registry package before consumer adoption.

`peer_rpc::v2::PeerRpcDiagnosticV2::from_wire` validates the existing authoritative error matrix before preserving code, phase, retry and correlation with a trusted `CoreId`. `PeerRpcDiagnosticActionV2` gives a controlled suggestion; unknown codes are redacted and non-retryable. Authenticate/tenant-filter export and never automatically replay an ambiguous command. The existing wire error schema is unchanged; this contract has no implementation dependency.

## Release candidate — 2026-10-14

`2.0.0-alpha.4`: source changes.


## Exact service authority — unpublished source

`ServiceLeaseAuthority` is a new additive contract in `appcore-distributed-contracts::control_plane::lease_authority`. Acquisition, exact renewal, exact release, inspection and check use the server clock. `FileControlPlane` implements it: TTL 1–30000 ms, existing same-Core acquisition returns the live grant unchanged, renewal cannot reacquire expiry, and changed grants are rejected. Data writer and update coordinator use separate deployment-selected `ServiceId`s. Heartbeat, role flags and Gateway HA instance epochs confer no data-write authority.

On restart use `FileControlPlane::open_existing_with_clock`; it rejects missing/corrupt state and never initializes a new epoch domain. Initial provisioning via `with_clock` is an explicit deployment operation. Authority operations use a nonwaiting OS lock and report contention/unavailability rather than queue indefinitely. `with_fenced_service_write` checks the exact lease and runs a trusted bounded local commit under that same lock; takeover cannot overlap that commit even if expiry occurs during it. The callback must not reenter the authority and the host owns its I/O bounds and application idempotency. A remote check response is only an observation: a remote store must atomically enforce fencing at commit. Do not cache a successful check or treat transport loss as permission.

Gateway adds `make_service_lease_router(gateway, authority, replay)` as explicit composition, absent from the default router. POST `/v1/gateway/service-lease` accepts `ServiceLeaseRequestV1` (version=1, identity, service_id, action); actions are acquire/renew/release/inspect/check. Authentication is mandatory even in insecure-testing. The isolated `service_lease_token_claims` domain requires exact command/query name, Core subject, full request hash and a single-use JTI of at most 96 bytes. TTL of a credential cannot exceed the requested acquisition lifetime or the grant expiry for grant-bound actions. Relay tokens, altered scope/action/epoch or wildcard credentials do not authorize it. The issuer must independently authorize tenant/cluster/Core/service/action before signing; clients must not receive the signing secret.

Compose a dedicated durable `PeerNonceStore` shared across restarts and authority replicas, for example `FilePeerNonceStore` on certified storage. No default is inferred. Bodies are limited to 16 KiB and only eight operations are admitted, without an admission queue. Authentication/replay I/O and authority calls run on blocking workers outside the async polling thread. Deployment owns provider I/O deadlines, graceful draining, TLS, shared clock policy and state custody. Response codes: 401 missing credentials, 403 invalid/replayed credentials, 409 stale/conflicting grant, 503 unavailable/contended authority or saturation. A rejected request never becomes an offline success. Provider failure or a lost response is ambiguous: obtain fresh authorized inspection, never guess success or rerun a write under a new epoch automatically.

Limits: a Core is the writer identity, not a PID; sharing Core credentials shares authority. Provision distinct Core identities for concurrent writers and retain a stable logical service for discovery. This reference is not consensus, not an HA arbiter with independent disks and not a remote storage fencing implementation. Total loss/rollback of authority or replay history requires controlled recovery/new authority domain; deleting files or calling the initializing constructor is forbidden as a recovery shortcut. V1 wire/storage and existing APIs remain unchanged. Consumer wiring, registry publication and real DNET/two-device partition qualification remain pending.
