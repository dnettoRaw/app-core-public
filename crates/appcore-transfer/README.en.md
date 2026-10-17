# appcore-transfer

Release candidate **`appcore-transfer 1.0.0-rc.1`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.


Beta `0.1.0-beta.1` transfers immutable opaque objects from explicitly approved sources. Descriptors must come from a trusted application or deployment. SHA-256 integrity does not authenticate a release.

[Guide](wiki/guide.en.md) · [Basic](wiki/examples/basic.en.md) · [Intermediate](wiki/examples/intermediate.en.md)

Default: object 64 MiB, block 64 KiB, store 256 MiB, 16,384 files. Hard maxima: object 256 MiB, block 1 MiB, 4,096 blocks, 65,536 files, 16 sources, 8 workers, 32 attempts/block. Assembly needs space for both blocks and complete content. Descriptor JSON is bounded to 1 MiB. Each worker retains at most one block; descriptor and inventory are bounded. Local filesystem operations require deployment-owned I/O bounds.

Enable peer-rpc to use PeerBlockSource and TransferDispatcher over existing signed Peer RPC V2 streams. The repository must authorize tenant, cluster, origin, target and object. Configure client I/O timeouts within the cooperative transfer deadline. Providers must honor cancellation; workers are always joined. Discovery and relays remain deployment-owned; Sync owns replication and Update owns authenticity, activation and rollback.

## Windows elevated-token creation and lifetime guards

This section describes unpublished source changes. Verify the exact registry package before consumer adoption.

Only newly created private files/directories receive an explicit current-user owner and protected owner-only DACL before strict validation. Existing insecure paths remain rejected. Windows roots and active object sessions retain Security directory guards to exclude ancestor/object-directory replacement during operations. Security is a Windows-only dependency; `peer-rpc` remains optional and no Update dependency is introduced. Native tests must run with limited and elevated tokens; fixture roots use non-verbatim Windows Temp paths. A verified object path after store drop is still deployment-owned and revalidated when opened.

## Release candidate — 2026-10-14

`0.1.0-beta.2`: source changes.

## Current source policy (unpublished source)

`AuthorizedBlockSource`, `TransferSourceScopeV1` and `TransferSourceAuthorizer` add an explicit policy boundary around a selected provider. Scope binds tenant, cluster, requester Core and source Core (128-byte ASCII identity profile); the callback receives the complete descriptor and `TransferControl`. Implementations must verify current object/redistribution permission, expiry/revocation and authority availability within the remaining deadline. Missing authority fails closed. Discovery remains deployment-owned: resolve bounded authenticated advertisements/Core identities to endpoints, authorize the exact descriptor and select at most 16 distinct scoped providers. Advertised availability or SHA-256 integrity is not redistribution permission.

The wrapper checks policy before requesting each block and again before the engine accepts buffered bytes. A policy denial quarantines that source for this invocation and rotates to another selected provider, preserving previously verified blocks. If all sources deny, `transfer` returns `TransferError::Unauthorized`; a single denial cannot count as a second approved source. Existing retry/concurrency/store limits remain. `quarantined_sources` now also includes policy denials. Peer RPC V2 maps typed authorization and identity rejection to Unauthorized, invalid stream hashes to Integrity and transport failures to Source, without logging remote details or changing the protocol.

Use wrappers through `transfer`, whose block buffer discards bytes on final policy failure. A direct arbitrary sink can already have received bytes before the second check. Checks are observations, not atomic remote revocation; an already authorized in-flight request cannot be retroactively prevented. The authenticated server's `TransferRepository::open_authorized` remains mandatory for object access, independently of client policy. No implicit endpoint/protocol fallback or installation is added.

These APIs are additive unpublished source. `TransferError` is non-exhaustive; authorization failures previously collapsed into generic exhaustion can now return Unauthorized. Update consumer diagnostics explicitly. Persistent formats, wire V2 and existing `SourceBinding::approved` remain unchanged; that lower-level constructor is still a trusted caller assertion. Native Windows limited/elevated ACL execution and real DNET evidence are separate qualification; local tests do not prove them.

Source checks govern new provider reads. Completion entirely from already verified local blocks does not contact or reauthorize providers; deployment separately authorizes local cache use and every server-side redistribution request.
