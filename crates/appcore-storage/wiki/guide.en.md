# appcore-storage

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** generic storage contracts and the bounded local file
provider.

**Internal dependencies:** `appcore-contracts`, `appcore-dnt`,
`appcore-security`, `appcore-types`.

**Primary API:** `StorageProvider`, `Repository`, `Migration`, `Transaction`,
health/status/errors, validated repository and migration IDs,
`FileStorageProvider`, storage manifests, V1 backup manifest/descriptor,
authenticated remote storage request/response helpers, and optional DNT-backed
sealed object, snapshot and secret stores.

The sealed file adapter writes normal DNT by default and exposes
`DntFileObjectStore::write_object_compact` for compressible snapshots, backups
and exported domain files. Compact writes remain ordinary DNT envelopes over
the same file provider; they do not change the storage backend contract.
Sealed reads derive a complete-envelope bound from `SealedStoragePolicy` and
reject oversized files before allocating the file buffer.

Use it when an application or Runtime service needs the documented local-first
storage profile. Keep domain schemas and tables outside. Unsupported
transactions fail explicitly.

Housekeeping and backup traversal is iterative and bounded and never follows
symbolic links or Windows reparse points. Backup listings use persisted
snapshot timestamps, with filesystem creation/modified metadata only for
single-file backups. Final file opens use platform no-follow semantics and are
revalidated under the process lock. The one-process profile still assumes an
owner-protected root: a hostile same-account process replacing an ancestor
directory during an operation remains outside this portable boundary.

For explicit post-1.0 preflight, `StorageCapabilityDescriptorV1` uses seven
closed guarantees and a catalog capped at 32 providers. A deployment lists
exact requirements in the storage provider setting `required_capabilities`.
The existing `storage.shared=true` application requirement adds `multi_host`.
Unknown, duplicate, unavailable and unsupported requirements return typed,
redacted errors before storage opens; there is no fallback. The built-in file
descriptor supplies only `snapshot`.

[Clean-source capability preflight evidence](benchmarks/storage-capability-v1-2026-08-26.en.md)

**Maturity:** stable RC contracts; file provider certified for one local process
and a filesystem with required lock/sync/rename semantics.
