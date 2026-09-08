# appcore-provider

[Português](README.pt.md) | [Français](README.fr.md)

`appcore-provider` defines how a Deployment Manifest selects concrete Runtime
infrastructure without making contract crates depend on implementations. It
owns provider roles, construction plans, factories, secret resolution, generic
coordination stores, fenced shared leases, and job-provider contracts.

## Composition flow

1. A deployment declares provider IDs and settings.
2. The composition root registers explicit `ProviderFactory` implementations
   in `ProviderRegistry`.
3. `DeploymentProviderPlan` validates that each required `ProviderRole` has
   exactly one usable provider.
4. The factory receives a bounded `ProviderContext` and resolved secret
   owners, then returns the requested implementation.

Missing or invalid providers stop bootstrap. The registry never substitutes a
provider silently. `ResolvedSecret` zeroizes its owned bytes and must not be
copied into diagnostics.

```rust
use appcore_provider::{
    CoordinationStoreProvider, InMemoryCoordinationStore, ProviderResult,
};

fn check_coordination() -> ProviderResult<u64> {
    let store = InMemoryCoordinationStore::default();
    store.ensure_compatible()?;
    store.schema_version()
}
```

## Coordination and lease guarantees

The in-memory coordination store is for embedded/test control planes. The file
store uses schema V2, atomic replacement, and a 4 KiB metadata/restore-source
limit. Readers check length before allocation, retain a sentinel byte to detect
growth, and reject symlinks, non-regular files, invalid UTF-8, and corrupt
records.

Filesystem leases persist a per-resource epoch high-water sidecar before
publishing an active lease. Release never resets the fencing sequence. A token
protects a write only when the writer checks the current epoch immediately
before that write; filesystems without reliable locks, rename, directory sync,
or cache coherence cannot provide strong split-brain protection.

Provider-specific SDK code belongs in an integration crate, not here. See the
[basic example](wiki/examples/basic.en.md), the
[intermediate example](wiki/examples/intermediate.en.md), and the
[crate guide](wiki/guide.en.md).

```bash
cargo test -p appcore-provider
```

## Stable documentation

Stable ID: **ACR-019**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-019). This permanent ID
remains valid if the wiki page moves.
