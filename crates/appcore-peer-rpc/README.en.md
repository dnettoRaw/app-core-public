# appcore-peer-rpc

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Authenticated direct peer client, HTTP host, validation and nonce persistence.

Peer requests bind protocol, tenant, cluster, source, target, expiry, nonce,
payload hash and signature. Private networking does not replace these checks.

Peer request, response and outbound DTO `Debug` output never includes opaque
payload bytes, idempotency values, nonce values or remote error details. HTTP
request and response debug output reports body length and redacts credentials.

Use `PooledPeerRpcTransport` to reuse bounded per-origin connections.
`StdPeerRpcTransport` remains the one-shot V1 compatibility transport.

The opt-in `v2` frame contract plus `PeerRpcChunkEncoder` and
`PeerRpcChunkAssembler` process large sources and sinks one bounded chunk at a
time. Default limits are 64 KiB decoded per chunk, 96 KiB encoded, 64 MiB total
and 1,024 chunks. Sequence, exact lengths, per-chunk hash, aggregate hash,
deadline, cancellation and post-decompression quota fail closed. These codec
APIs do not select V2 transport automatically; V1 routes never infer V2.

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
body is bound to a fresh bearer token and processed incrementally. Binary
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

```bash
cargo test -p appcore-peer-rpc
```
