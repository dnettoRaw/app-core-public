# Native page — minimum use

The runnable Rust example opens validated native text and a button, drains a bounded activation queue and closes within its fixed lifetime. Deployment owns startup, pumping and shutdown.

```sh
cargo run -p appcore-ui --features native-macos --example native_page
```

[Rust](../../examples/native_page.rs) · [Guide](../guide.en.md)

These examples are included in the public beta package for macOS. Other platform renderers remain future work.
