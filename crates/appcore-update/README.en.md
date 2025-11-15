# appcore-update

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Opaque artifact selection, authenticity, staging, activation, health gate and
rollback.

The Runtime validates application/Runtime/protocol identity, checksum and trust
without understanding application code. Schema migration remains
application-owned.

File providers bound reads and reject non-regular files. Activation revalidates
staged size and SHA-256, then installs immutable build artifacts without
replacing an existing build path; only exact-byte idempotent reuse is accepted.

```bash
cargo test -p appcore-update
```
