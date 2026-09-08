# appcore-provider

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** implementation-neutral provider factories, registry,
deployment plans, coordination/job contracts and secret resolution.

**Internal dependencies:** `appcore-contracts`.

**Primary API:** `ProviderRole`, `ProviderContext`, `ProviderFactory`,
`ProviderRegistry`, `DeploymentProviderPlan`, provider errors/results,
zeroizing `ResolvedSecret` and `SecretProvider`; coordination schema V2,
in-memory/file coordination stores; shared-resource leases with fencing;
generic job spec/lease/completion/provider.

File coordination metadata and restore sources are limited to 4 KiB. Readers
check the declared file length before allocation, retain one sentinel byte to
detect growth, and reject symlinks, non-regular files and invalid UTF-8. The
schema V2 format and atomic replacement behavior are unchanged.

Filesystem leases use a per-resource lock file, an atomically replaced
versioned state file and a versioned epoch high-water sidecar. The sidecar is
persisted before publishing an active lease and survives release, restart and
interrupted acquisition, so an epoch is never reused. The epoch is a fencing token only
for writers that check it before writing. Shared filesystems that do not
provide reliable lock, rename, directory sync or cache-coherence semantics
cannot provide strong split-brain protection through this adapter alone.

Use it to compose explicit deployment providers. Do not register silent
fallbacks or put provider-specific SDK code in this crate.

**Maturity:** stable composition RC surface; distributed jobs remain outside the
first 1.0 operational profile.
