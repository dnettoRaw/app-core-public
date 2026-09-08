# appcore-types

[Português](README.pt.md) | [Français](README.fr.md)

`appcore-types` supplies the validated vocabulary shared by AppCore contracts
and Runtime components. It replaces unchecked strings at process, storage, and
wire boundaries with small types that enforce the same rules everywhere.

## Main contracts

- application, tenant, cluster, node, Core, instance, capability, command,
  query, event, state, and sync-group identifiers;
- `RuntimeIdentity` and `CoreIdentity`, including compatibility policy and
  status;
- `CapabilityDescriptor`, distributed Core manifests, and peer endpoints;
- `TraceContext` for tenant-aware trace, span, parent, command, and origin
  correlation;
- `RuntimeError` and `RuntimeResult` for controlled foundational failures.

Create identifiers at the first untrusted boundary and pass the typed value
afterward. A trace child preserves its trace, tenant, origin, and command
correlation while receiving a new span and current Core.

```rust
use appcore_types::{CoreId, RuntimeResult, TenantId, TraceContext};

fn child_trace() -> RuntimeResult<TraceContext> {
    let api = CoreId::new("core-api")?;
    let root = TraceContext::new(
        "trace-42",
        "span-api",
        api.clone(),
        api,
        TenantId::new("tenant-a")?,
    )?;

    root.child_span("span-worker", CoreId::new("core-worker")?)
}
```

## Boundaries and failures

This crate owns no I/O, mutable Runtime state, provider behavior, or business
model. Invalid length, character set, reserved spelling, or inconsistent
identity is rejected during construction instead of crossing into another
subsystem. Protocol and contract versions remain explicit values; they are not
inferred from a network peer.

See the [basic example](wiki/examples/basic.en.md), the
[intermediate example](wiki/examples/intermediate.en.md), and the
[crate guide](wiki/guide.en.md). Run:

```bash
cargo test -p appcore-types
```

## Stable documentation

Stable ID: **ACR-003**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-003). This permanent ID
remains valid if the wiki page moves.
