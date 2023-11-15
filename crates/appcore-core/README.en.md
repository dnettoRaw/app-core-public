# appcore-core

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Generic in-process Runtime lifecycle, command/event dispatch, state, decisions,
audit and idempotency.

`RuntimeBuilder` and `AppPlugin` remain low-level compatibility contracts. New
applications use `appcore_bin::application::Application` and
`run_application`.

This crate contains no product domain, HTTP server or provider-specific code.

```bash
cargo test -p appcore-core
```
