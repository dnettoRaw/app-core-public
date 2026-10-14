# Web page — Runtime workflow

Build the typed client, then run the SDK fixture with a declared capability catalog and explicit surface authorization. It executes a query, an idempotent command and a subscription. Replaying the same key keeps one effect and one Runtime event. Close rejects pending requests and releases the surface.

```sh
cargo run -p appcore-ui --features web-macos --example web_bridge
```

[Rust](../../examples/web_bridge.rs) · [SDK](../../examples/web-bridge/runtime.rs) · [Client](../../examples/web-bridge/page.js) · [Guide](../guide.en.md)

The fixture session is only for qualification. A real deployment must authorize its own principal, tenant and capabilities and isolate blocking providers. It starts no hidden executor.

These examples are included in the public beta package for macOS. Other platform renderers remain future work.

```sh
cargo run -p appcore-ui --features native-macos --example native_gallery
cargo run -p appcore-ui --features native-macos --example native_counter
cargo run -p appcore-ui --features web-macos --example web_runner
cargo run -p appcore-ui --features web-macos --example web_3d
```

The gallery uses Editing → Review → Applied, revision checks, locked controls and scripted typed commands. No AI model is connected. The runner has deterministic physics, collision/score/restart/pause. The 3D example uses direct system WebGL, perspective/depth and six Lorenz trajectories, RK4 integration, 24,576 bounded point samples, orbit/zoom, trail isolation, pause and single-step controls; it is not a Rust-native 3D engine.
