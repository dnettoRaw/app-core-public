# appcore-storage

Release candidate **`appcore-storage 1.0.5-rc.2`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.

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

Remote auth-storage V1 separates its bounded representations: 256 KiB maximum
plaintext for `seal`, 384 KiB sealed data for `open`, 1 MiB authenticated token
body and 64 KiB HTTP headers. Oversized input fails before hex/JSON expansion;
client response parsing reuses its owned buffer instead of cloning the body.

The sealed file adapter writes normal DNT by default and exposes
`DntFileObjectStore::write_object_compact` for compressible snapshots, backups
and exported domain files. Compact writes remain ordinary DNT envelopes over
the same file provider; they do not change the storage backend contract.
Sealed reads derive a complete-envelope bound from `SealedStoragePolicy` and
reject oversized files before allocating the file buffer.

`FileStorageProvider::read_bytes` materializes at most 64 MiB and still reads
through `max + 1` after checking metadata, so concurrent growth cannot bypass
the limit. Single-file backup streams at most 1 GiB into an exclusive temporary
file, syncs it and atomically renames it; failure removes the temporary and
keeps the former destination. A complete snapshot accepts at most 1 GiB per
file and 16 GiB in aggregate. These ceilings are exported as
`DEFAULT_FILE_READ_MAX_BYTES`, `MAX_STORAGE_BACKUP_FILE_BYTES` and
`MAX_STORAGE_SNAPSHOT_BYTES`.

The complete-snapshot manifest has a 16 MiB ceiling. Its V1 pretty JSON is
serialized directly through a bounded 16 KiB writer into an exclusive atomic
temporary file, and deserialized through a bounded 16 KiB reader. Complete
encoded manifest buffers no longer coexist with the decoded file inventory;
exact-limit input remains valid and one non-retained byte detects growth.

Use it when an application or Runtime service needs the documented local-first
storage profile. Keep domain schemas and tables outside. Unsupported
transactions fail explicitly.

`StorageWriteBarrier` coordinates writers with update installation: acquire an
`open` permit, call `block_new_writers`, then `drain` with a deadline. Use
`release` after a successful drain, or `seal_after_install_start` once
installation begins. Nested permits and bounded owner snapshots are supported;
sealed state is never cleared automatically in-process.

Housekeeping and backup traversal is iterative and bounded and never follows
symbolic links or Windows reparse points. Backup listings use persisted
snapshot timestamps, with filesystem creation/modified metadata only for
single-file backups. Final file opens use platform no-follow semantics and are
revalidated under the process lock. The one-process profile still assumes an
owner-protected root: a hostile same-account process replacing an ancestor
directory during an operation remains outside this portable boundary.

Tree traversal visits at most 200,000 entries incrementally while retaining
only the bounded 16,384-directory work stack and consumer-owned results.
Snapshot creation keeps its required sorted file paths without a second global
entry list; health retains only a counter, cleanup only matching temporary
paths, and symlink validation no entries at all. The depth ceiling remains 128.
Snapshot verification also counts actual files incrementally and borrows the
previous manifest path while checking order; it does not build a second path
inventory or clone one path per entry.

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

## Candidate bootstrap and individual atomic files

Unpublished source additions: after blocking and draining writes, deployment
may call `authorize_candidate_bootstrap("candidate")` once. Reserve before
`seal_after_install_start`, then consume the authority with `run`. Its callback
receives a scoped `CandidateBootstrapPermit` with no clone/nesting/escape API.
Normal `open` remains rejected; success, error or panic leaves the barrier
sealed. Dropping unused authority revokes it. Deployment owns candidate
selection and filesystem permissions; this is process-local admission, not a
cross-process token, sandbox or filesystem authorization mechanism.

`AtomicFileWriter::new(absolute_path, max_bytes)?.write(bytes)` replaces one
file outside the managed root. Existing real parent directories are required;
symlinks/reparse points and traversal are rejected. The budget is 1 byte to
64 MiB, input is checked before staging, and exclusive sibling files are synced
before rename. Unix output uses mode 0600 and parent-directory sync; other
platforms retain the existing provider durability limits. Parent directories
must be deployment-protected. Concurrent writers are last-successful-rename
wins; this is not a transaction. A post-rename sync failure can leave new bytes
visible while reporting failure.

An application backup format needs an application-owned import bridge: bound
and authenticate/decrypt the original archive using its original codec, reject
unknown versions/unsafe paths, validate its inventory and stage verified opaque
files into a new provider snapshot. Validate the snapshot before switching
roots, preserve the old root for rollback and test interrupted imports. Do not
rename an old archive into a Runtime snapshot or add application-format decoding
to AppCore.

## Release candidate — 2026-10-14

`1.0.5-rc.1`: dependency metadata alignment.
