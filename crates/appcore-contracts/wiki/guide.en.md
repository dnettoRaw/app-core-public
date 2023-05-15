# appcore-contracts

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** stable, implementation-independent Runtime manifests and
policies.

**Internal dependencies:** none.

**Primary API:** `ApplicationManifestV1`, `DeploymentManifestV1`,
`DeploymentManifestBuilder`, `RuntimeManifestV1`, `RuntimeMode`,
`RuntimeOperationalMode`, capability/storage/leadership/job/scheduler/health/
update/module policies, provider/network/TLS/volume/environment configuration,
`ContractError`.

Use it to parse, build and validate portable contracts. Keep serialized names
and meanings stable. Do not add transport, filesystem, process or business
implementation code.

**Maturity:** stable RC contract surface. V1 changes must be additive and
compatible for the 1.0 line.
