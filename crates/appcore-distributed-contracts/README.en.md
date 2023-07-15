# appcore-distributed-contracts

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Versioned control-plane and peer RPC wire/provider contracts.

It defines registration, presence, heartbeat, discovery, service leases, peer
envelopes, advertisements and provider traits. HTTP, storage, authentication
and concrete coordination live in implementation crates.

Opaque content envelopes and Peer RPC request/response payloads serialize
unchanged, but their `Debug` implementations expose lengths and routing
metadata rather than application-owned bytes or error details.

```bash
cargo test -p appcore-distributed-contracts
```
