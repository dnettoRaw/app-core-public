# appcore-storage

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Generic storage contracts, bounded local file provider and backup format.

Application schemas and data models remain application-owned. Unsupported
transactions fail explicitly. The file profile expects one local process and a
filesystem with reliable locks, sync and atomic rename.

Housekeeping and backup traversal is iterative and bounded and never follows
symbolic links or Windows reparse points. Backup listings use persisted
snapshot timestamps, with filesystem creation/modified metadata only for
single-file backups. Final file opens use platform no-follow semantics and are
revalidated under the process lock. The one-process profile still assumes an
owner-protected root: a hostile same-account process replacing an ancestor
directory during an operation remains outside this portable boundary.

The post-1.0 `StorageCapabilityDescriptorV1` contract describes transactions,
locking, snapshots, streaming, online backup, multi-process and multi-host
guarantees without naming provider internals. `required_capabilities` is an
explicit deployment setting; unknown, duplicate or unsupported requirements
fail before startup. The file provider advertises only `snapshot`. Frozen V1
manifest shapes and existing non-shared V1 deployments are unchanged.

```bash
cargo test -p appcore-storage
```
