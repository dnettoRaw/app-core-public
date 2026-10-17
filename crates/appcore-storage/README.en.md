# appcore-storage

Release candidate **`appcore-storage 1.0.5-rc.2`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Generic storage contracts, bounded local file provider and backup format.

**Responsibility:** generic storage contracts and a bounded local file
provider.

**Internal dependencies:** `appcore-contracts`, `appcore-dnt`, `appcore-security`
and `appcore-types`.

**Main API:** `StorageProvider`, `Repository`, `Migration`,
`Transaction`, health/status/errors, validated IDs, `FileStorageProvider`,
storage manifests, V1 backup, authenticated remote-storage helpers and
optional DNT-sealed stores for objects, snapshots and secrets.

Remote auth-storage V1 accepts at most 256 KiB of plaintext for `seal`, 384 KiB
of sealed data for `open`, a 1 MiB authenticated token body and 64 KiB of HTTP
headers. These exported limits reject input before hex/JSON expansion; the
default 256 KiB seal/open roundtrip is covered end to end.

The sealed-file adapter writes normal DNT by default and exposes
`DntFileObjectStore::write_object_compact` for compressible snapshots, backups
and exported application files. Compact writes remain ordinary DNT envelopes
on the same file provider; the storage backend contract does not change.
Sealed reads derive the complete-envelope limit from `SealedStoragePolicy`.
Files already exceeding that limit at the metadata check are rejected before
allocating the file buffer; growth during reading is bounded and rejected
after reading.

`FileStorageProvider::read_bytes` materializes at most 64 MiB and rejects a
larger or concurrently growing file. Single-file backups stream through an
exclusive temporary file, accept at most 1 GiB, sync before atomic rename and
preserve the previous backup on failure. Complete snapshots are limited to 1
GiB per file and 16 GiB in aggregate. The exported constants define these
ceilings.

The complete-snapshot manifest has a 16 MiB ceiling. Its V1 pretty JSON is
serialized directly through a bounded 16 KiB writer into an exclusive atomic
temporary file, and deserialized through a bounded 16 KiB reader. Complete
encoded manifest buffers no longer coexist with the decoded file inventory;
exact-limit input remains valid and one non-retained byte detects growth.

Application schemas and data models remain application-owned. Unsupported
transactions fail explicitly. The file profile expects one local process and a
filesystem with reliable locks, sync and atomic rename.

`StorageWriteBarrier` coordinates storage writers with update installation.
Call `open` for a bounded writer permit, `block_new_writers`, then `drain` with
a deadline before installation. `release` reopens a drained non-sealed barrier;
`seal_after_install_start` permanently blocks new writers until process restart.
Nested permits are supported and `snapshot` reports bounded owner labels without
clearing a stuck owner automatically. This is admission coordination, not a
claim of database transaction semantics.

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

The post-1.0 `StorageCapabilityDescriptorV1` contract describes transactions,
locking, snapshots, streaming, online backup, multi-process and multi-host
guarantees without naming provider internals. `required_capabilities` is an
explicit deployment setting; unknown, duplicate or unsupported requirements
fail before startup. The file provider advertises only `snapshot`. Stable V1
manifest shapes and existing non-shared V1 deployments are unchanged.

**Maturity:** stable RC contracts; the file provider is certified for one
local process on a filesystem with the required lock/sync/rename semantics.

```bash
cargo test -p appcore-storage
```

## Stable documentation

Stable ID: **ACR-011**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-011). This permanent ID
remains valid if the wiki page moves.

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
