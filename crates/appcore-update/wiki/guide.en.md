# appcore-update

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** opaque application artifact selection, authenticity,
staging, activation, health gate and rollback.

**Internal dependencies:** contracts and provider.

**Primary API:** artifact descriptor and signing payload; authenticity verifier,
feature-gated unsigned-local and Ed25519 implementations, trust policy/key status; update
request/provider and file provider/factory; staged artifact, activation receipt
and file store; coordinator, preparation/outcome, health check and fault
injection contracts.

Use it for application binaries or opaque artifacts. The Runtime validates
identity, version, protocol, checksum and trust but never understands
application code or schema.

`ReleaseCatalog` is an additive catalog contract for signed platform targets.
Use `ArtifactTarget` for the operating system, architecture and host format;
the target is covered by the descriptor signature. A catalog validates every
entry before it can be selected, rejects duplicate identity/channel/target/
version keys, and never supplies its own trust roots. V1 descriptors remain
usable outside the catalog.

For peer-driven selection, use `LatestCompatibleOfferRequestV2` and
`LatestCompatibleOfferResponseV2`. Selection excludes equal or older versions
and checks target, channel, Runtime version and protocol. The peer does not
replace local trust policy: call `verify_descriptor` with the deployment's
`ArtifactAuthenticityVerifier` before accepting the descriptor.

Use `ReleaseCatalogStore` when local bytes are published beside a catalog. Its
entries keep the safe relative location outside the signed descriptor, and it
serves chunks only when the complete descriptor is present in the validated
catalog. The controlled root rejects traversal, symlinks, non-regular files
and ambiguous duplicate hashes.

Host activation is expressed by `ActivationAdapter`: implement `prepare`,
`activate`, `healthcheck`, `commit`, `rollback` and `recover` for the selected
deployment. `ActivationRequest` and `ActivationEvidence` convert to the V2
receipt boundary; platform installers remain outside AppCore and recovery
actions are explicit.

For bounded transfer, implement `ArtifactSource` and `ArtifactWriter`, then use
`receive_artifact`. The receiver validates the requested offset and chunk bound,
the exact declared size and the final SHA-256. Receiving is deliberately
separate from staging, activation and installation.

Use `UpdateCache` for resumable download staging. It owns `.part` files,
hash-addressed objects, descriptor metadata, a process lock and a quota. It
does not activate artifacts or remove protected releases; `FileArtifactStore`
remains the installation and rollback store. With `secure_permissions`, it
rejects symlinked or dangerously writable directories and ancestors without
repairing them. The reusable handle/ACL boundary lives in `appcore-security`,
while the layer wall keeps this crate independent from it.

For Peer RPC V2, carry `ArtifactOfferRequestV2` and
`ArtifactChunkRequestV2` as bounded request payloads and repeat the response
metadata with `ArtifactOfferResponseV2` or `ArtifactChunkResponseV2`. These
payloads contain no remote path. Use the existing V2 stream for the bytes;
authorization and transport framing stay outside this crate.

For recovery, create an `ActivationReceiptV2` in `FileRecoveryStore`, call
`inspect_recovery` during startup, and apply only an explicit `RecoveryAction`.
`replay` fences the action by `attempt_id` and digest. The host must perform
and observe any external rollback before recording it; V1 recovery semantics
remain unchanged.

Use `QuarantineStore` after a failed health or activation decision. Its bounded
key includes application, channel, version, build and digest. Quarantine is
durable, survives downgrade and restart, and can be cleared only through the
explicit `release` operation. `quarantine_until` supports an exclusive expiry;
`select_with_quarantine_report` explains active exclusions with their reason,
build and SHA-256 key. Expired entries are retained for diagnostics but no
longer block selection.

Run `appcore-update-diagnose --json descriptor <file>`, `receipt <directory>`,
`quarantine <directory>` or `cache <directory>` for CI-safe inspection. The
tool never activates, repairs, releases or removes anything. Exit classes are
stable: `64` usage, `65` invalid data, `66` missing input and `74` I/O.

The repository fixtures cover corrupt and partial cache inputs, ambiguous
catalog selection and incomplete recovery receipts. The evidence harness also
exercises the process lock under bounded concurrent quarantine writes; native
filesystem durability remains platform-specific release evidence.

File reads preflight size before allocation, use a fixed 16 KiB scratch buffer
plus one non-retained sentinel byte, and reject non-regular final components.
Activation streams staged size and SHA-256 validation through a fixed 64 KiB
buffer, then hard-links the staged file to an immutable build path. An existing
path is reused only when its size and digest match the descriptor exactly; it
is never replaced. Atomic final-component no-follow is
implemented on Unix. Other platforms retain metadata checks but require their
deployment filesystem boundary to prevent reparse races.

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

**Maturity:** stable RC lifecycle; remote supply chains require signed
provenance and deployment trust roots.

For mobile, construct `MobileUpdatePolicy` with the deployment-owned action,
minimum cluster requirement and required protocol. Evaluate the installed
version, candidate, cluster and target before announcing availability. A
protocol mismatch blocks the client; a stale cluster blocks the offer. The
contract reports policy only and never performs store, MDM or sideload actions.
