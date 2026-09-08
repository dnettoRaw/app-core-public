# appcore-peer-rpc

V1, JSON V2 and binary V2 body ingress share 16 slots per host, including
separately requested routers from that host. Saturation returns HTTP 503 before
body collection; reception expires after 10 seconds (408). V1 retains its
2 MiB HTTP body ceiling; V2 retains the registry frame ceiling. Health and
manifest bypass body admission. These are transport-level failures before
frame decoding, not signed V2 replies. Raw admitted bodies are bounded by
16 times the largest enabled body ceiling, not total process RSS. Decoding,
decompression, dispatch and response memory have separate limits.

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Authenticated direct peer client, HTTP host, validation and nonce persistence.

**Responsibility:** authenticated peer client, HTTP host, validation and replay
protection.

**Internal dependencies:** core, distributed contracts, security and transport.

**Main API:** token issuer/authenticator/dispatcher traits and HashToken/static
implementations; memory/file nonce stores; configuration, validator and hashes;
retry/client configuration and transport trait; pooled and standard one-shot
transports; HTTP state and host.

Peer requests bind protocol, tenant, cluster, source, target, expiry, nonce,
payload hash and signature. Private networking does not replace these checks.

Peer request, response and outbound DTO `Debug` output never includes opaque
payload bytes, idempotency values, nonce values or remote error details. HTTP
request and response debug output reports body length and redacts credentials.

Use `PooledPeerRpcTransport` to reuse bounded per-origin connections.
`StdPeerRpcTransport` remains the one-shot V1 compatibility transport.
Both transports take ownership of the HTTP DTO body allocation. Uncompressed
V1 and exact V2 bodies move into `HttpRequest` without a full clone; V1 creates
a separate gzip buffer only when compression is selected and smaller.
The V1 client moves each owned outbound payload into one envelope and reuses
that owner across bounded retries. Every retry still refreshes timestamp,
expiry, nonce, signature binding, and encoded HTTP body. With a 4 MiB raw
payload, five Apple M1 release processes kept p50 within +0.08%, reduced peak
RSS by 7.21%, workload RSS delta by 9.02%, and retained delta by 7.99%.
On ingress, `decode_peer_rpc_envelope_json` checks the encoded bound and
deserializes an uncompressed V1 body directly from its borrowed HTTP bytes.
The composition root then moves the decoded payload allocation into
`CommandEnvelope`; neither boundary retains a second complete payload.

The memory and file nonce stores independently reject identifiers above 128
bytes. The file store decodes its owner-only V1 state through a reader capped
at 16 MiB and serializes the retained map directly through a fixed 64 KiB
buffer into an exclusive temporary file. Complete corruption, unknown fields,
invalid keys and oversized state fail closed; failed writes remove their stage.
Windows replacement uses the atomic write-through platform operation.

`BoundedReplayStore` applies the same 128-byte nonce validation and caps both
live entries and estimated retained bytes. Its derived default never exceeds
32 MiB; `with_max_bytes` selects a tighter ceiling and `memory_metrics` exposes
current, peak, maximum and rejection counters without revealing nonce values.

V1 and V2 host dispatch share 16 process-wide blocking permits. The host pool
uses at most 16 threads with 1 MiB stacks and retires idle threads after five
seconds. Saturation fails before Tokio queue admission; V2 reports
`CapacityExceeded`.

The opt-in `v2` frame contract plus `PeerRpcChunkEncoder` and
`PeerRpcChunkAssembler` process large sources and sinks one bounded chunk at a
time. Default limits are 64 KiB decoded per chunk, 96 KiB encoded, 64 MiB total
and 1,024 chunks. Sequence, exact lengths, per-chunk hash, aggregate hash,
deadline, cancellation and post-decompression quota fail closed. These codec
APIs transfer an incompressible chunk's owned allocation from source to frame
to receiver without cloning it. A fixed stack-only full-chunk probe skips
speculative gzip for likely already-compressed data; compressible chunks still
use gzip. These APIs do not select V2 transport automatically; V1 routes never
infer V2.

`PeerRpcStreamRegistry` adds exact session and decoded-byte admission quotas,
exclusive owner-only request spools, bounded dispatcher response pulls and
observable saturation/cleanup counters. Every error, cancellation, expiry and
completion path releases its partial file and reservation.
Unix requires the effective owner with directory/file modes `0700`/`0600`.
Windows rejects reparse points and any allow ACE outside the current process
owner SID. Unsupported platforms reject the spool configuration.

V2 HTTP is installed only by `PeerRpcHttpHost::with_v2_stream_registry`.
JSON remains the default codec. A host additionally calls
`with_v2_binary_codec`, and a client calls `with_stream_codec_v2(Binary)`, to
use the distinct Postcard routes and native chunk bytes. Each selected exact
body is bound to a fresh bearer token and processed incrementally. Canonical
JSON is serialized directly into SHA-256 for that binding, so signing does not
retain a second complete encoded body beside the frame. The public
`json_payload_hash` helper provides the same byte-exact path to dependants.
Binary
bodies are capped at 256 KiB and are never HTTP-compressed; per-chunk bounded
gzip remains part of the signed frame. Missing or mismatched binary support is
terminal and never falls back to JSON. Open
frames reuse tenant, cluster, target, trace, deadline and nonce-replay checks;
commands require idempotency. Frames are not retried after ambiguous transport
failure. V1 remains the default host surface and never upgrades automatically.

V2 host rejections carry the validated `PeerRpcWireErrorV2` matrix. The client
rejects contradictory code/phase/retry metadata and normalizes unknown codes
to a redacted, non-retryable outcome. V2 frames still never retry after an
ambiguous acknowledgement. V1 clients decode only the host's exact controlled
strings; only exact availability/capacity codes enter bounded retry.

[Clean-source 64 MiB V2 certification evidence](wiki/benchmarks/peer-rpc-v2-2026-08-26.en.md)

Use only when tenant, cluster, source, target, protocol, expiry, nonce and
integrity can be established. `AllowPeerAuthenticator` is test-only.

**Maturity:** V1 is stable; the certified post-1.0 V2 transport remains in
development.

```bash
cargo test -p appcore-peer-rpc
```

## Stable documentation

Stable ID: **ACR-017**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-017). This permanent ID
remains valid if the wiki page moves.
