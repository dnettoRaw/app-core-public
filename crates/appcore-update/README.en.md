# appcore-update

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Opaque artifact selection, authenticity, staging, activation, health gate and
rollback.

**Responsibility:** opaque artifact selection, authenticity, staging,
activation, health gate and rollback.

**Internal dependencies:** `appcore-contracts` and `appcore-provider`.

**Main API:** artifact descriptors and signing payloads, authenticity
verifiers, trust policies and signing-key status, update requests and providers,
file factories, staged artifacts, activation receipts and stores, coordination,
preparation and outcomes, health checks and fault injection. Ed25519 verifies
signed artifacts. Unsigned local artifacts require the explicit
`allow-unsigned-local-artifacts` feature and an owner-controlled local root;
they are not a remote supply-chain fallback.

The Runtime validates application/Runtime/protocol identity, checksum and trust
without understanding application code. Schema migration remains
application-owned.

File providers preflight size before allocation, read through a fixed 16 KiB
scratch buffer plus one non-retained sentinel byte, and reject non-regular
files. Activation streams the staged file through a fixed 64 KiB SHA-256
buffer instead of materializing it, then installs immutable build artifacts
without replacing an existing build path; only exact-size, exact-digest
idempotent reuse is accepted. Installation uses a hard link to the immutable
build path. Atomic no-follow protection of the final path component is available
on Unix. Other platforms retain metadata checks but rely on the deployment's
filesystem boundary against reparse races.
Active/previous pointers and pending activation receipts borrow their
descriptors, pass a non-retaining 1 MiB sizing check, and serialize directly to
the atomic temporary file through a fixed 16 KiB buffer. Their V1 JSON encoding
is unchanged. Reads also deserialize directly through a fixed 16 KiB bounded
reader instead of retaining a complete encoded byte vector beside the decoded
pointer or receipt. Missing files, I/O failures and decode failures remain
distinct so the pending-activation upgrade wall is preserved.

The file provider streams the bounded index once and retains only the best
semantic version and its descriptor. Each descriptor is validated and then
discarded or selected while the JSON array is decoded, so neither a descriptor
vector nor a sorted candidate list is retained. Equal versions preserve the
first index entry. A fixed 16 KiB reader, 1 MiB preflight and one non-retained
sentinel byte reject declared oversize and concurrent growth.

**Maturity:** stable RC lifecycle; a remote supply chain requires signatures,
provenance and trust roots.

```bash
cargo test -p appcore-update
```

## Stable documentation

Stable ID: **ACR-021**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-021). This permanent ID
remains valid if the wiki page moves.
