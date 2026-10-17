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

The source 3D showcase now also includes a deterministic 12,000-star galaxy, a procedural terrain and a vertex-shader wave surface (8,192 triangles each). Scene selection, pause/step/reset and orbit/zoom share the bounded WebGL renderer. These are local example capabilities, not native Rust GPU compute, physical fluids or a 4K performance claim.

The source gallery starts in Fractal city: 391 boxes (4,692 triangles), three recursive tower levels, skybridges, directional face lighting and fog. Click the canvas to focus; WASD flies, Q/E changes altitude, Shift boosts, drag looks around, R returns home and Esc releases focus. Flight has bounded coordinates and no solid collision. Keys clear on focus/visibility/context loss. This finite recursive example runs in WebGL; native Rust 3D remains planned.

The optional appcore-ui feature native-input-timing adds local event timing without changing UiNativeActivation or UI V1. next_timed_activation returns callback-to-observation timing and, only inside correlated mouse/keyboard pump dispatch, event-to-observation timing. Uncorrelated accessibility callbacks remain None. Correlation is scoped and restored across nested dispatch/unwinding; invalid clocks do not produce a fabricated zero. Mouse-down timing includes hold/tracking until release. Raw uptime is never serialized or logged.

NativeRenderer.frame_progress reports generation-local submitted/completed frame IDs through queue callbacks; counters never wrap or regress. Completion is GPU queue work observed by polling, not compositor presentation or display scanout. The bounded native_meshes probe retains 256 samples/markers, reports overflow and outstanding markers, and correlates actions with the first subsequently submitted frame; intermediate states may coalesce. Presentation-call return and observed completion are separate endpoints. The optional timing profile uses systemUptime for local elapsed time; bundle examples/native-gpu/PrivacyInfo.xcprivacy (reason 35F9.1) with the application according to [Apple's API reasons](https://developer.apple.com/documentation/bundleresources/app-privacy-configuration/nsprivacyaccessedapitypes/nsprivacyaccessedapitype).

Qualification: Metal, Apple M1, physical 1024×768, 320 instances/2 meshes, pump wait 8 ms. Ten coordinate clicks through native events and ten accessibility activations delivered 20 actions. Native-event p50/p95/p99: action 1.549/19.964/19.964 ms; present-call return 13.326/30.874/30.874 ms; observed GPU completion 23.511/42.509/42.509 ms (n=10). Mixed callback-to-action 2.289/103.954/104.105 ms (n=20); accessibility had no invented event timestamps (uncorrelated=10). All overflow/pending counters were zero; submitted=completed=3435, retained_windows=0. Small-sample results on this profile do not qualify 4K60 or universal input latency.

```sh
cargo run -p appcore-ui --features gpu-macos,native-input-timing --example native_meshes
```

GpuMeshScene::new_lod now composes fixed LOD batches through separate prepass/final selection buffers, a shared fresh Hi-Z pyramid and conservative all-level occlusion bounds. encode_lod takes LodSceneFrame with explicit matching matrix/view/groups; using the static encode method for an LOD scene is rejected before writes. Geometry remains duplicated per pass and included in admission. set_lod_resident updates availability consistently in both passes; no packed geometry is evicted. Static scene construction/encoding stays unchanged. NativeRenderer exposes enable_mesh_lods, present_mesh_lods and drained-frame availability changes under existing surface ownership, frame permits, resize peak admission and completion tracking.

native_lod adds 256 refined spheres/octahedron proxies and 64 boxes/tetrahedron proxies, orbit/zoom/pause, pixel budgets 4/12/32/96 and logical Fine on/off. A pure reducer rejects unknown commands; camera-to-projection alignment and bounded controls are tested. Real Metal 512×512 tests with two LOD groups verify detail loss/restoration, empty reset, admission and explicit scene mode. Native process ran to its 180-second deadline: frames=3239, submitted=completed=3239, retained_windows=0. This run had actions=0. Visual/input/resize qualification of this new example remains pending: native automation returned cgWindowNotFound for this app and the previously qualified control app. This does not qualify 4K60, streaming, meshlet compilation or memory eviction.

```sh
cargo run -p appcore-ui --features gpu-macos,native-input-timing --example native_lod
cargo test -p appcore-render --features metal --test lod_scene -- --include-ignored
```


The native_lorenz, native_meshes and native_lod examples include a readable performance HUD: successful presents/second, mean/p95 inter-present interval (128 samples), CPU present-call time, process resident RAM (RSS), managed GPU payload, physical resolution, adapter and submitted/completed frame counters. Values refresh twice per second. CPU submit is not GPU execution time; GPU payload is budget accounting, excluding driver/compositor/extra swapchain allocations, not measured VRAM. These metrics do not prove display scanout or 4K60.

Click below the HUD to activate camera controls: drag to orbit, W/S or wheel to zoom, A/D or left/right arrows to orbit, up/down arrows to tilt. Escape, Tab, control clicks and window focus loss release navigation and clear held keys. OS shortcuts pass through. This is bounded orbit/zoom, not free flight or object manipulation. Manual navigation stops automatic mesh orbit; Resume restarts it. Lorenz simulation remains independent. The viewers stay open until closed; optionally set E3DN_DEMO_SECONDS=30 (1–600) for a bounded run.

With gpu-macos, enable_scene_input(overlay_height) opts a graphics window into fixed-state navigation; take_scene_input(elapsed) drains deltas, caps elapsed at 50 ms and reports focus without exposing raw key text. native_process_resident_bytes reads current process RSS through the system Mach API; failures remain explicit. Label backgrounds over graphics are opaque for readability.
