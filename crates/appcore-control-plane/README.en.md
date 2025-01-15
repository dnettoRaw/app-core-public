# appcore-control-plane

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Presence, heartbeat, discovery and lease implementations for distributed
Runtime operation.

Available profiles include in-memory, offline, file and bounded HTTP clients.
The control plane coordinates Runtime infrastructure and never stores business
payloads.

The file profile bounds persisted state and backups to 16 MiB. Lease expiry and
epoch arithmetic fail closed on overflow; an exhausted epoch is never reused as
a fencing token.

```bash
cargo test -p appcore-control-plane
```
