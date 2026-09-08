# appcore-distributed-contracts

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
