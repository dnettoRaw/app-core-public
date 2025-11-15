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

File reads are bounded and reject non-regular final components. Activation
revalidates staged size and SHA-256, then hard-links the staged file to an
immutable build path. An existing path is reused only when its bytes match the
descriptor exactly; it is never replaced. Atomic final-component no-follow is
implemented on Unix. Other platforms retain metadata checks but require their
deployment filesystem boundary to prevent reparse races.

**Maturity:** stable RC lifecycle; remote supply chains require signed
provenance and deployment trust roots.
