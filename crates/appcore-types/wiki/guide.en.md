# appcore-types

Release candidate **`appcore-types 1.0.4-rc.1`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** validated foundational identifiers, identity and trace
types shared across Runtime contracts.

**Internal dependencies:** `appcore-contracts`.

**Primary API:** application, node, tenant, cluster, Core, instance, command,
event, query, state and capability IDs; `RuntimeIdentity`, `CoreIdentity`,
version policies/status, `TraceContext`, `RuntimeError`,
`RuntimeResult`.

Use these types instead of passing unchecked strings across boundaries. Do not
place implementation state, I/O or provider behavior here.

**Maturity:** stable foundational RC surface.
