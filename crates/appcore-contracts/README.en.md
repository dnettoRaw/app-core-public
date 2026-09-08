# appcore-contracts

[Português](README.pt.md) | [Français](README.fr.md)

`appcore-contracts` defines the versioned documents that applications,
deployments, and running AppCore hosts exchange. It is the place to depend on
manifest and policy types without importing a Runtime implementation.

## What it owns

- `ApplicationManifestV1`: portable application identity, modules,
  capabilities, and Runtime requirements;
- `DeploymentManifestV1`: installation mode, providers, network, TLS, volume,
  environment, secret-reference, Supervisor, and watchdog configuration;
- `RuntimeManifestV1`: the identity, operational mode, and health reported by
  one running host;
- validated identifiers and policies for resources, storage, leadership,
  scheduling, jobs, health, and updates.

The three manifests have different owners. An application publishes its
Application Manifest, an operator supplies a Deployment Manifest, and the host
produces a Runtime Manifest. Keeping them separate prevents machine-specific
configuration from leaking into a portable application artifact.

## When to use it

Use this crate when a tool or application must construct, parse, validate, or
inspect AppCore V1 contracts. Constructors and `validate` reject malformed
identifiers, missing requirements, and inconsistent policy combinations.

```rust
use appcore_contracts::{
    ApplicationId, ApplicationManifestV1, ContractResult,
    RuntimeRequirements, ServiceId,
};

fn manifest() -> ContractResult<ApplicationManifestV1> {
    ApplicationManifestV1::new(
        ApplicationId::new("notes-app")?,
        "1.0.0",
        "Notes",
        "example-vendor",
        ServiceId::new("notes")?,
        RuntimeRequirements::new("1.0.0", "1")?,
    )
}
```

## Boundaries

This standalone contract crate contains no I/O, provider implementation,
listener, process lifecycle, or business schema. V1 serialized names are a
compatibility wall: compatible additions may evolve in V1, but removed or
incompatible inputs are not guessed or silently converted.

See the [basic example](wiki/examples/basic.en.md), the
[intermediate example](wiki/examples/intermediate.en.md), and the
[crate guide](wiki/guide.en.md). Run its focused tests with:

```bash
cargo test -p appcore-contracts
```

## Stable documentation

Stable ID: **ACR-002**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-002). This permanent ID
remains valid if the wiki page moves.
