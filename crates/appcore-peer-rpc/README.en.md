# appcore-peer-rpc

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Authenticated direct peer client, HTTP host, validation and nonce persistence.

Peer requests bind protocol, tenant, cluster, source, target, expiry, nonce,
payload hash and signature. Private networking does not replace these checks.

Peer request, response and outbound DTO `Debug` output never includes opaque
payload bytes, idempotency values, nonce values or remote error details. HTTP
request and response debug output reports body length and redacts credentials.

```bash
cargo test -p appcore-peer-rpc
```
