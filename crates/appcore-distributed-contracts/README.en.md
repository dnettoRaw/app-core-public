# appcore-distributed-contracts

Release candidate **`appcore-distributed-contracts 1.0.2-rc.1`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.


[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Versioned control-plane and peer RPC wire/provider contracts.

**Responsibility:** versioned control-plane and Peer RPC wire/provider
contracts.

**Internal dependencies:** `appcore-contracts`, `appcore-types`.

**Main API:** protocol constants and paths; registration, presence, heartbeat,
peer directory, compatibility and service leases, leadership decisions and
traits; peer paths, envelopes, responses, errors, call kinds, advertisement
DTOs, client executor and transport metadata for opaque content envelopes.

It defines registration, presence, heartbeat, discovery, service leases, peer
envelopes, advertisements and provider traits. HTTP, storage, authentication
and concrete coordination live in implementation crates.

Opaque content envelopes and Peer RPC request/response payloads serialize
unchanged, but their `Debug` implementations expose lengths and routing
metadata rather than application-owned bytes, nonce/idempotency values or
remote error details.

`OpaqueEnvelopeDeduplicator` retains one shared allocation per accepted message
ID across its membership and acceptance-order indexes. Its bounded FIFO
eviction and duplicate decisions are unchanged. Retaining 65,536 distinct
128-byte IDs measured 32.83 ms p50 and 27.25 MiB peak RSS on Apple M1, down
from 37.55 ms and 35.86 MiB with duplicate strings. Transport validation and
deduplication reject empty IDs, control characters and IDs above
`MAX_OPAQUE_MESSAGE_ID_BYTES` (1,024 UTF-8 bytes) before retention.

Peer RPC V2 is a separate opt-in chunk-frame family under `peer_rpc::v2`.
Open, chunk, commit and cancel frames declare exact protocol, identity,
sequence, decoded sizes, deadline and integrity. Encoded chunk bytes use one
canonical base64 JSON string, never an integer array. Human-readable encoding
emits that string through a fixed 3 KiB input/4 KiB output scratch buffer, and
decoding borrows the encoded JSON string when the deserializer supports it.
The exact wire is unchanged. V1 remains only under `peer_rpc::v1`;
implementations must never infer or convert between them.

V2 also defines an explicitly selected binary codec. It uses a fixed magic,
codec version, message kind and exact body length around a bounded Postcard
payload; chunk bytes remain native bytes instead of base64. JSON is unchanged,
and a binary frame or reply is capped at 256 KiB before decoding. Codec
mismatch is an error, never an automatic fallback.

V2 rejections use `PeerRpcWireErrorV2`: a fixed code, authoritative phase and
retryability, a bounded retry hint/correlation identity and a protocol-owned
redacted message. Unknown codes normalize to one terminal `unknown` outcome.
The stable V1 string rejection has a separate exact decoder and never uses
substring matching.

**Maturity:** V1 is stable; the V2 chunk contract is opt-in post-1.0 work in
progress.

```bash
cargo test -p appcore-distributed-contracts
```

## Stable documentation

Stable ID: **ACR-006**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-006). This permanent ID
remains valid if the wiki page moves.

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
