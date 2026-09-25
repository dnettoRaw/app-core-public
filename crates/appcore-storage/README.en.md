# appcore-storage

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
