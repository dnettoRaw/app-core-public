# Application envelope V1 to Peer RPC streaming V2

Application envelope/schema versions, Runtime sync wire versions and Peer RPC
transport versions are independent. Moving bytes through streaming V2 does not
upgrade a domain schema or change leader-to-follower semantics. Existing chunks
and streams already exist; migration needs explicit application adapters.

## Mapping and deployment contract

| Application boundary | Streaming boundary | Preserved responsibility |
| --- | --- | --- |
| V1 negotiation / status | Versioned application capability query | Advertise supported transport explicitly; absence means not eligible for V2 |
| `IngestChunkV1` | Verified open/chunk/commit stream carrying application payload | External adapter reconstructs/validates application bytes, identity, sequence and digest |
| `PullRequestV1` / `PullChunkV1` | Explicit command/query and bounded response pull frames | Preserve cursor, checkpoint and authorization semantics outside frame transport |
| Snapshot pages / application events | Bounded opaque payloads | Application validates schema and publishes only a complete verified result |

The external protocol package must own the actual mapping and golden fixtures.
Keep maintained application V1 endpoints available for old application clients
only in that external package. Do not teach Runtime V1 endpoints to infer V2,
parse removed envelopes or convert application formats. Removed Runtime inputs
still return `NO MORE SUPPORTED PLEASE UPDATE`.

## Staged migration

1. Inventory deployed envelope/schema versions, identities, scopes, byte limits,
   retry behavior and persisted outboxes/checkpoints. Record golden V1 fixtures
   in the application repository; never copy business schemas into Runtime.
2. Add an explicitly versioned application capability to the external adapter.
   Enable `PeerRpcStreamRegistry` on an existing owner-only spool directory and
   install it with `PeerRpcHttpHost::with_v2_stream_registry`. Set per-capability
   byte/deadline limits; verify tenant, cluster, origin and target before dispatch.
3. Keep maintained application V1 clients on their explicit existing path.
   Select V2 only after validated application capability negotiation or an
   explicit deployment setting. The default Runtime host remains V1-only.
   Canonical JSON V2 is separate from binary framing: binary needs host
   `with_v2_binary_codec` and client
   `with_stream_codec_v2(PeerRpcStreamCodecV2::Binary)` together.
4. Prove identical application payload/result and checkpoint behavior through
   each selected transport. Test timeout, cancellation, replay, duplicate
   command, corruption, oversize, isolation, peer loss and ambiguous commit.
   Never retry V2 frames automatically after an ambiguous transport failure;
   reconcile application receipts using the same idempotency key before retry.
5. Roll out receiver adapters first, then selected senders. Monitor safe error
   codes, admission saturation, spool cleanup and checkpoint lag. Keep rollout
   cohorts explicit; do not silently downgrade on authentication or media errors.
6. Roll back by stopping new V2 sends, draining/expiring sessions, reconciling
   committed command receipts and preserving outboxes/checkpoints. Resume the
   explicitly selected maintained application V1 adapter only if its schema and
   persisted state remain compatible. Incompatible persistence needs a reviewed
   version/migration/rollback plan; do not erase state or invent conversion.

## Acceptance evidence

Required application-owned evidence: old client/new receiver, new sender/new
receiver, explicitly selected old sender/old receiver, and controlled refusal
for unsupported pairs. Require byte-equivalent golden payloads, no duplicate
publication on idempotent replay, no permanent pending reservations on failure,
no partial snapshot publication, bounded resource use and restart recovery.
Runtime tests certify transport primitives, not an unimplemented external
mapping. No application adapter is supplied by this document.
