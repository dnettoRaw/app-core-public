# appcore-peer-rpc

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** authenticated direct peer client, HTTP host, validation and
replay protection.

**Internal dependencies:** core, distributed contracts, security and transport.

**Primary API:** token issuer/authenticator/dispatcher traits and HashToken or
static implementations; in-memory/file nonce stores; validation config,
validator and signing/payload hashes; retry/client config and transport trait;
pooled and standard one-shot transports; HTTP state and host.

Use `PooledPeerRpcTransport` to reuse bounded per-origin connections.
`StdPeerRpcTransport` preserves the V1 one-shot `Connection: close` behavior.

Use it only after tenant, cluster, source, target, protocol, expiry, nonce and
payload integrity can be established. `AllowPeerAuthenticator` is for tests,
not remote production.

Peer request, response, outbound and HTTP DTO `Debug` output reports payload
lengths and omits opaque bytes, credentials, nonce/idempotency values and remote
error details.

For explicitly selected protocol V2, `PeerRpcChunkEncoder` reads one bounded
chunk from a `Read` source and emits open/chunk/commit frames;
`PeerRpcChunkAssembler` verifies and writes one decoded chunk to a `Write` sink.
The default aggregate limit is 64 MiB and no frame can exceed its decoded or
encoded quota. Missing, duplicate, reordered, corrupt, expanded-over-quota,
expired or cancelled input permanently closes that assembler. A failed finish
drops the sink rather than exposing partial bytes as committed data.

`PeerRpcStreamRegistry` owns partial V2 sessions under explicit session and
decoded-byte quotas. It spools requests into exclusive files in an existing
owner-only directory, dispatches only fully verified payloads and serves
responses through explicit bounded pull frames. Error, cancellation, expiry
and completion remove the owned file and reservation. Its snapshot reports
active sessions, reserved bytes, saturation and cleanup counters.
Unix validates the effective owner and `0700`/`0600` directory/file modes.
Windows rejects reparse points and every allow ACE outside the current process
owner SID. Unsupported platforms fail closed during registry construction.

Install HTTP V2 explicitly with
`PeerRpcHttpHost::with_v2_stream_registry`. The default host remains V1-only
and V2 defaults to canonical JSON. Binary framing requires the host's separate
`with_v2_binary_codec` opt-in and the client's
`with_stream_codec_v2(PeerRpcStreamCodecV2::Binary)` selection. It uses
distinct query/command paths and the exact
`application/vnd.appcore.peer-rpc.v2+postcard` media type. Each exact selected
body is authenticated and request/response bytes move one frame at a time.
Binary bodies are never HTTP-gzip encoded and remain below 256 KiB; optional
chunk gzip is still decoded under the declared limit. Missing routes, media
type mismatch and malformed replies are terminal without JSON fallback. Open admission validates
tenant, cluster, target, trace, deadline, command idempotency and nonce replay.
Frames are never retried after an ambiguous transport failure; cancellation is
best effort and deadline cleanup is authoritative.

V2 rejection bodies use `PeerRpcWireErrorV2`. The client validates code,
phase, retryability, retry delay, correlation and the protocol-owned message as
one matrix before returning `PeerRpcStreamClientErrorV2::Remote`. Unknown
codes are observable but terminal and redacted. V1 rejections become
`PeerRpcError::RemoteRejected` through exact code equality; availability and
replay-capacity are the only remote V1 retry cases. Neither path interprets a
substring.

V2 codec availability is not negotiation. Callers must select the V2 module
and transport explicitly. `/v1/peer/*` continues to parse only V1 and there is
no automatic fallback.

**Maturity:** stable V1 surface; certified post-1.0 V2 development transport.

[V2 bounded-stream certification evidence](benchmarks/peer-rpc-v2-2026-08-26.en.md)
