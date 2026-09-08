# appcore-peer-rpc

V1, JSON V2 and binary V2 body ingress share 16 slots per host, including
separately requested routers from that host. Saturation returns HTTP 503 before
body collection; reception expires after 10 seconds (408). V1 retains its
2 MiB HTTP body ceiling; V2 retains the registry frame ceiling. Health and
manifest bypass body admission. These are transport-level failures before
frame decoding, not signed V2 replies. Raw admitted bodies are bounded by
16 times the largest enabled body ceiling, not total process RSS. Decoding,
decompression, dispatch and response memory have separate limits.

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
Both consume the owned HTTP DTO body allocation: an uncompressed V1 body or an
exact V2 frame keeps the same `Vec<u8>` allocation through `HttpRequest` instead
of retaining a cloned body beside it.

The V1 client moves an owned outbound payload into one envelope and keeps that
owner through bounded retries. Each retry still generates fresh temporal and
nonce fields, signs the refreshed envelope, and encodes a new HTTP body. A
4 MiB raw-payload workload across five Apple M1 release processes kept p50
within +0.08%, reduced peak RSS by 7.21%, workload RSS delta by 9.02%, and
retained RSS delta by 7.99%.

On ingress, `decode_peer_rpc_envelope_json` enforces the encoded byte ceiling
before parsing and borrows an uncompressed HTTP body directly. The Runtime
composition root also moves the decoded V1 payload into `CommandEnvelope`.
With a 4 MiB raw payload on Apple M1, five release processes reduced p50 from
53.06 to 52.35 ms, peak RSS from 35.80 to 23.81 MiB and the workload RSS delta
by 39.52%.

V1 and V2 dispatch share 16 process-wide blocking permits and a Tokio pool
with the same ceiling, 1 MiB stacks and five-second idle retirement. A full
gate rejects before queue admission; V2 uses `CapacityExceeded`.

Use it only after tenant, cluster, source, target, protocol, expiry, nonce and
payload integrity can be established. `AllowPeerAuthenticator` is for tests,
not remote production.

Peer request, response, outbound and HTTP DTO `Debug` output reports payload
lengths and omits opaque bytes, credentials, nonce/idempotency values and remote
error details.

Use `FilePeerNonceStore` only on its owner-private directory. It accepts at most
65,536 live entries, bounds every nonce to 128 bytes and caps the V1 file at 16
MiB. Loading decodes directly from the limited reader; each accepted request
rewrites the ordered map through a fixed 64 KiB buffer and atomic replacement,
without an encoded JSON `Vec`. Startup rejects unknown fields, invalid keys,
complete corruption and oversized files. The crate benchmark validates the
maximum entry count with idle/workload/retained RSS phases.

`BoundedReplayStore` applies the same nonce validation to process-local replay
protection. Count and estimated retained bytes are both bounded; the default
byte ceiling is derived from the entry policy and never exceeds 32 MiB.
`with_max_bytes` can select a tighter ceiling. `memory_metrics` reports current,
peak and maximum bytes plus byte-pressure rejections without exposing nonces.

For explicitly selected protocol V2, `PeerRpcChunkEncoder` reads one bounded
chunk from a `Read` source and emits open/chunk/commit frames;
`PeerRpcChunkAssembler` verifies and writes one decoded chunk to a `Write` sink.
The default aggregate limit is 64 MiB and no frame can exceed its decoded or
encoded quota. Missing, duplicate, reordered, corrupt, expanded-over-quota,
expired or cancelled input permanently closes that assembler. A failed finish
drops the sink rather than exposing partial bytes as committed data. For an
identity chunk, encoder and assembler move the same owned allocation instead of
cloning the decoded bytes at either boundary. A fixed stack-only full-chunk
probe suppresses speculative gzip only when the input appears already
incompressible; structured compressible chunks still use gzip.

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
Canonical JSON is serialized directly into SHA-256 for the token binding, so
signing does not retain a second complete encoded body beside the frame.
Dependants can reuse that byte-exact path through `json_payload_hash`.
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
