# appcore-capabilities

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Descriptor catalog, local handler registry and deterministic local/remote
provider resolution.

`CapabilityCatalog` authorizes manifest-composed descriptors without claiming
that a handler exists. `CapabilityRegistry` owns executable local handlers.
Both catalog enforcement and provider resolution use the same mode,
idempotency, operational-write and leadership checks. The Runtime does not
infer product meaning from capability names.

```bash
cargo test -p appcore-capabilities
```
