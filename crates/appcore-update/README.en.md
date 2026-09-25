# appcore-update

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Opaque artifact selection, authenticity, staging, activation, health gate and
rollback.

The additive `ReleaseCatalog` contract validates bounded, signed
platform-specific descriptors and selects the greatest compatible version for
an `UpdateIdentity`. V1 descriptors without a target remain valid; catalog
entries require `ArtifactTarget`, whose fields are included in the signature
payload. Trust roots are always supplied by deployment policy.

`LatestCompatibleOfferRequestV2` and `LatestCompatibleOfferResponseV2` select
the greatest release newer than the installed version and compatible with the
target, Runtime version and protocol. The peer returns the signed descriptor;
the client must call `verify_descriptor` with its local trust policy before
staging or fetching bytes.

`ReleaseCatalogStore` opens a bounded JSON catalog below a controlled root.
Each entry keeps its safe relative byte location separate from the signed
descriptor. It rejects path traversal, symlinks, non-regular files, duplicate
hashes and descriptors not present in the catalog. The store implements
`ArtifactSource` and reads only the declared artifact size.

`ActivationAdapter` defines the host boundary for `prepare`, `activate`,
`healthcheck`, `commit`, `rollback` and `recover`. `ActivationRequest` and
`ActivationEvidence` are bounded and can become an `ActivationReceiptV2`;
desktop, raw-binary and Docker installers remain outside the Runtime. Adapter
failures must become explicit recovery actions and never trigger guessed
rollback.

`ArtifactSource`, `ArtifactWriter` and `receive_artifact` provide a synchronous,
bounded transfer path. The receiver checks offsets, chunk sizes, declared
length and the final SHA-256, and never activates or installs the result.

`UpdateCache` adds resumable hash-addressed staging with an exclusive process
lock, quota reservation, partial-file recovery and durable descriptor/object
publication. It is separate from `FileArtifactStore`. With
`secure_permissions`, the cache rejects symlinked or dangerously writable
directories and ancestors without repairing them. The reusable handle/ACL
boundary is exposed by `appcore-security`; the layer wall keeps this crate
from taking a direct dependency on it.

Peer transfer metadata is exposed as path-free `offer` and `chunk` contracts
for authenticated Peer RPC V2 streams. The transport remains responsible for
authentication, tenant/cluster isolation, deadlines, sequencing and decoded
byte hashes; `appcore-update` validates artifact identity, offsets and repeated
size/digest metadata.

The additive V2 recovery API uses `ActivationReceiptV2`,
`FileRecoveryStore`, `inspect_recovery` and explicit `replay` actions. It keeps
the V1 receipt unchanged, fences actions by attempt and digest, and never
performs external rollback implicitly.

`QuarantineStore` persistently records failed releases using the application,
channel, version, build and SHA-256 as one bounded key. Entries carry typed
reasons and timestamps, survive restart and downgrade, and remain active until
an explicit `release` call or their configured expiry. Use
`select_with_quarantine_report` for the selected descriptor plus bounded
exclusions containing the release key, reason and expiry.

The `appcore-update-diagnose` binary is read-only. It inspects a descriptor,
receipt directory, quarantine directory or cache directory with `--json` or
human output. JSON is versioned with `schema_version: 1`; artifact references,
signatures and host bindings are redacted.

The `fixtures/` tree and focused integration tests provide portable evidence
for partial/corrupt cache data, ambiguous catalogs, incomplete receipts and
bounded concurrent quarantine writes.

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

`MobileUpdatePolicy` evaluates a bounded mobile request before an offer is
announced. It distinguishes self-replace, store, MDM, deployment-assisted
sideload and unsupported actions, blocks obsolete protocols, and requires a
minimum cluster version. It never installs an artifact or recommends bypassing
App Store/Play Store policy; those actions remain deployment-owned.
