# Bounded verified transfer

Beta `0.1.0-beta.1` transfers immutable opaque objects from explicitly approved sources. Descriptors must come from a trusted application or deployment. SHA-256 integrity does not authenticate a release.

## Recovery and limits

A private existing root is required: Unix owner-only 0700, or a Windows owner-only ACL. The store holds a cross-process exclusive lock. Verified blocks survive restart; corruption and unknown versions fail closed. Recognized orphan staging files are removed under the lock. TTL cleanup removes only incomplete objects. Complete objects remain until deployment-owned retention.

Default: object 64 MiB, block 64 KiB, store 256 MiB, 16,384 files. Hard maxima: object 256 MiB, block 1 MiB, 4,096 blocks, 65,536 files, 16 sources, 8 workers, 32 attempts/block. Assembly needs space for both blocks and complete content. Descriptor JSON is bounded to 1 MiB. Each worker retains at most one block; descriptor and inventory are bounded. Local filesystem operations require deployment-owned I/O bounds.

## Transport and ownership

Enable peer-rpc to use PeerBlockSource and TransferDispatcher over existing signed Peer RPC V2 streams. The repository must authorize tenant, cluster, origin, target and object. Configure client I/O timeouts within the cooperative transfer deadline. Providers must honor cancellation; workers are always joined. Discovery and relays remain deployment-owned; Sync owns replication and Update owns authenticity, activation and rollback.

Windows cross-compilation is checked; native Windows recovery and physical mesh performance remain release qualification work. MCU remains planned. No installation or artifact activation occurs here.

## Windows elevated-token creation and lifetime guards

This section describes unpublished source changes. Verify the exact registry package before consumer adoption.

Only newly created private files/directories receive an explicit current-user owner and protected owner-only DACL before strict validation. Existing insecure paths remain rejected. Windows roots and active object sessions retain Security directory guards to exclude ancestor/object-directory replacement during operations. Security is a Windows-only dependency; `peer-rpc` remains optional and no Update dependency is introduced. Native tests must run with limited and elevated tokens; fixture roots use non-verbatim Windows Temp paths. A verified object path after store drop is still deployment-owned and revalidated when opened.

## Release candidate — 2026-10-14

`0.1.0-beta.2`: source changes.
