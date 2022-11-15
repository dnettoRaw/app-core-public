# appcore-types

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Validated IDs, Runtime/Core identity, compatibility, trace context and
foundational errors.

Use these types at contract boundaries instead of unchecked strings. The crate
depends only on `appcore-contracts` and contains no I/O or provider behavior.

```bash
cargo test -p appcore-types
```
