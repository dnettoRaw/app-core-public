# appcore-capabilities

Local tests:

```bash
cargo test -p appcore-capabilities
```

`CapabilityCatalog` applies the same count and version-byte limits. Its
`from_descriptors` stops consuming input at the first rejection; it does not
collect the entire input before validation.

The local `CapabilityRegistry` admits at most 4,096 handlers and descriptor
versions of 1–256 UTF-8 bytes. Rejection preserves registered handlers and uses
`HandlerRejected` with a bounded reason. `iter_descriptors` borrows descriptors
without cloning or allocating a collection; order is unspecified. These limits
do not control memory internal to external handlers or their descriptor method.


**Responsibility:** catalog descriptors, register local handlers and resolve
compatible local or remote providers.

**Internal dependencies:** contracts, core and distributed contracts.

**Main API:** catalog and enforcement context, request/response/error, local
handler and remote invoker traits, local provider, registry, provider
selection, resolution policy, selection trait/default, resolver and the Peer
RPC invoker based on the distributed contract.

Descriptor catalog, local handler registry and deterministic local/remote
provider resolution.

`CapabilityCatalog` authorizes manifest-composed descriptors without claiming
that a handler exists. `CapabilityRegistry` owns executable local handlers.
Both catalog enforcement and provider resolution use the same mode,
idempotency, operational-write and leadership checks. The Runtime does not
infer product meaning from capability names.

The default resolver scans discovery records by reference, keeps only the
first compatible fallback and clones only the provider it selects. It does not
materialize every compatible peer or clone the peer's complete capability-name
list after its descriptor already matched. A custom
`CapabilitySelectionPolicy` still receives the complete owned candidate slice
required by its stable public contract.

Use `CapabilityResolver::handle_owned` when the caller owns a request selected
for local or remote execution. Local handlers keep their borrowed contract;
the Peer RPC invoker moves the request ID, capability, payload, idempotency key
and trace directly into the outbound request. `handle` and
`RemoteCapabilityInvoker::invoke_remote` remain compatible for borrowed
callers and implementations.

Default `handle`, `handle_local` and `handle_owned` execution also borrow the
selected registry provider or discovery record through enforcement and
dispatch. `resolve()` deliberately keeps returning an owned
`CapabilityProvider`, and custom selectors keep receiving their complete owned
candidate list.

**Maturity:** stable RC routing profile.

## Stable documentation

Stable ID: **ACR-016**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-016). This permanent ID
remains valid if the wiki page moves.
