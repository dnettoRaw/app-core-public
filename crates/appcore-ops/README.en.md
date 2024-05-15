# appcore-ops

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Vendor-neutral health, heartbeat, logging, metrics, observations and
availability. Managed-service compatibility APIs are reexported from
`appcore-supervisor`; new lifecycle code should depend on that crate directly.

Signals, queues and files are bounded. Provider-specific exporters and
application business metrics remain outside this Runtime crate.

```bash
cargo test -p appcore-ops
```
