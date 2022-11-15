# appcore-types

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
