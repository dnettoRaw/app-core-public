# appcore-distributed-contracts

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Versioned control-plane and peer RPC wire/provider contracts.

It defines registration, presence, heartbeat, discovery, service leases, peer
envelopes, advertisements and provider traits. HTTP, storage, authentication
and concrete coordination live in implementation crates.

Opaque content envelopes and Peer RPC request/response payloads serialize
unchanged, but their `Debug` implementations expose lengths and routing
metadata rather than application-owned bytes or error details.

Peer RPC V2 is a separate opt-in chunk-frame family under `peer_rpc::v2`.
Open, chunk, commit and cancel frames declare exact protocol, identity,
sequence, decoded sizes, deadline and integrity. Encoded chunk bytes use one
canonical base64 JSON string, never an integer array. V1 remains only under
`peer_rpc::v1`; implementations must never infer or convert between them.

V2 also defines an explicitly selected binary codec. It uses a fixed magic,
codec version, message kind and exact body length around a bounded Postcard
payload; chunk bytes remain native bytes instead of base64. JSON is unchanged,
and a binary frame or reply is capped at 256 KiB before decoding. Codec
mismatch is an error, never an automatic fallback.

V2 rejections use `PeerRpcWireErrorV2`: a fixed code, authoritative phase and
retryability, a bounded retry hint/correlation identity and a protocol-owned
redacted message. Unknown codes normalize to one terminal `unknown` outcome.
The frozen V1 string rejection has a separate exact decoder and never uses
substring matching.

```bash
cargo test -p appcore-distributed-contracts
```
