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
