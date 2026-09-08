# appcore-capabilities

`CapabilityCatalog` applies the same count and version-byte limits. Its
`from_descriptors` stops consuming input at the first rejection; it does not
collect the entire input before validation.

The local `CapabilityRegistry` admits at most 4,096 handlers and descriptor
versions of 1–256 UTF-8 bytes. Rejection preserves registered handlers and uses
`HandlerRejected` with a bounded reason. `iter_descriptors` borrows descriptors
without cloning or allocating a collection; order is unspecified. These limits
do not control memory internal to external handlers or their descriptor method.

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

Default selection is allocation-bounded by the selected result: discovery is
scanned through borrowed peer and descriptor references, only the first
compatible fallback is retained, and only the selected provider is cloned.
Compatibility checks use the descriptor that already matched instead of
rescanning a copied list of every advertised name. A custom
`CapabilitySelectionPolicy` keeps the stable behavior and receives the complete
owned candidate slice.

When the caller no longer needs a request, use
`CapabilityResolver::handle_owned`. Resolution and policy checks still borrow
the request; a selected local handler keeps the stable borrowed contract, while
the Peer RPC invoker transfers all owned request fields into its outbound DTO.
`RemoteCapabilityInvoker::invoke_remote_owned` has a borrowed default so
existing custom invokers remain source compatible.

The three execution methods borrow the default selected local provider or
discovery record until dispatch completes. This avoids cloning a peer's
identity, endpoints, capabilities and metadata for a transient call.
`resolve()` still returns an owned provider, while a custom selector retains
the complete owned-candidate contract.

**Maturity:** stable RC routing profile.
