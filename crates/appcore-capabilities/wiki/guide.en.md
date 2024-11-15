# appcore-capabilities

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** catalog composed capability descriptors, register local
handlers and resolve compatible local or remote providers.

**Internal dependencies:** contracts, core and distributed contracts.

**Primary API:** descriptor catalog and enforcement context, capability
request/response/error, local handler and remote invoker traits, local provider,
registry, provider selection, resolution policy and selection trait, default
deterministic selection, resolver and contract-backed peer RPC remote invoker.

Use generic capability IDs and explicit requirements. The resolver may consider
health, mode, leadership and policy; it must not interpret product semantics.

Use `CapabilityCatalog` when a composition root needs to resolve and authorize
manifest descriptors before dispatch. Use `CapabilityRegistry` only when a real
local handler is available. Catalog and resolver share request, write-mode and
leadership enforcement, so a host does not need to rescan manifests locally.

**Maturity:** stable RC routing profile.
