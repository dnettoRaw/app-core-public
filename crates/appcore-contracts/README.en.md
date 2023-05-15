# appcore-contracts

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Stable, implementation-independent AppCore manifests and policies.

Main contracts: `ApplicationManifestV1`, `DeploymentManifestV1`,
`RuntimeManifestV1`, Runtime modes, capability declarations and
storage/leadership/job/scheduler/health/update policies.

This foundation crate has no internal AppCore dependency. It must not contain
I/O, provider implementations or business concepts.

```bash
cargo test -p appcore-contracts
```
