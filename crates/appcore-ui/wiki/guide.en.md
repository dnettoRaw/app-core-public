# appcore-ui

Release candidate **`appcore-ui 1.0.0-rc.1`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.

Independent UI module, release candidate `1.0.0-rc.1`.

The module is intended to render native pages defined in Rust and web pages
written in HTML/TypeScript. Window lifecycle, rendering, actions, visual state
and the Runtime bridge belong to appcore-ui. Business layouts and schemas remain
application-owned. Deployment explicitly selects the platform backend, supplies
authentication and owns process startup and shutdown; the SDK starts no UI host.

The existing implementation provides bounded UI V1 command/query/event contracts,
capability enforcement, authenticated surface ownership, cancellation, deadlines,
backpressure and a typed TypeScript client. Commands retain Runtime-owned
idempotency and event emission. These bridge tests do not prove rendering.

The native and local web previews below render on macOS. The Runtime bridge can be attached explicitly. The external screen has been verified in an isolated synthetic profile. Other platforms remain future work. Prefer operating-system facilities, reuse existing
contracts and justify every external dependency. No external application shell
or bundled rendering framework is required by this module.

UI V1 is independent of manifest and peer protocol versions. Unsupported inputs
fail with `NO MORE SUPPORTED PLEASE UPDATE`. Queued UI notifications are bounded
and ephemeral; reconstruct state by query after reconnect or reported loss.
Cancellation does not roll back an already performed effect.

## Native source preview

`UiNativePageV1` declares native controls from Rust. `UiNativePage::new`
(or `decode`) validates version 1, unique identifiers, finite top-left geometry,
512 controls, 256 KiB encoded content, 4096 bytes per label and at most 4,194,304
logical viewport pixels. This initial profile uses fixed frames; it is not a
complete responsive layout system. Debug does not expose page content.

On macOS, the explicitly selected `native-macos` feature exposes
`UiNativeApplication` and `UiNativeWindow`. Deployment creates one application
controller with a physical window budget (1–8), opens validated pages, pumps
bounded event batches (first wait at most one second, then up to 64 events with
a cooperative 2 ms drain budget), and drains trusted native
activations. Each window's queue admits 1–256 activations. Overflow and caught
close-hook panics have counters. Activations identify application bindings;
they do not bypass Runtime authorization or dispatch commands themselves.
Close invokes the supplied hook once; Drop releases retained windows and controls.
Closed handles retain their physical window slot until dropped.

The backend uses three optional direct system bindings (`objc2`,
`objc2-app-kit`, `objc2-foundation`), five active packages including transitive bindings.
They provide typed OS API calls rather than another rendering framework.
Unsafe code is isolated to documented foreign-call/callback boundaries. These
packages are not required by the default bridge or page model. Other native platform backends remain future work.

Run the source preview on macOS:

```sh
cargo run -p appcore-ui --features native-macos --example native_page
```

The preview has a bounded lifetime, displays native text and a button, emits
only action/control identifiers, and closes normally. A real macOS window was
visually inspected; its button delivered `state.inspect` and closing exited 0.
The external screen qualification below is a separate consumer check.

## Local web resources

`UiWebBundle` consumes application-built immutable HTML, compiled JavaScript, CSS,
PNG/JPEG and WOFF2 resources. It performs no filesystem or network access. Paths
are explicit canonical relative ASCII identifiers; traversal, encoded paths, URL
input and query strings are rejected. Resource lookup has no fallback route.
The bundle admits 512 assets, 2 MiB per asset and 16 MiB total retained bytes, with
explicit MIME types and UTF-8 validation for text. These limits bound retained
source bytes, not decoded image/font or platform engine memory.

The initial policy denies remote connections, inline scripts/styles, frames,
forms and plugins. The platform preview below enforces CSP and
rejects foreign navigation; the data model alone is not a web renderer. The external screen qualification below verifies this policy in an isolated consumer. No additional dependency was added for this
resource boundary.

## Direct web source preview

On macOS, the explicit `web-macos` feature adds `UiWebWindow` using the system
web component, one optional platform binding (`objc2-web-kit`) and its typed
callback-block binding (`block2`). No browser runtime or application shell is
bundled. The default crate remains free of these platform dependencies.

The host serves only declared bundle resources on a random per-window local
origin. It emits Content-Security-Policy and nosniff headers, rejects foreign
navigation/popups/frames and URL credentials/ports/queries/encoding, and uses a
nonpersistent data store. At most 16 resource tasks are admitted; cancellation
is checked between native response callbacks. Engine loss or initial loading
over 30 seconds closes the window when the deployment continues pumping.
No automatic retry or process host is started. This is not an engine-memory bound.

```sh
cargo run -p appcore-ui --features web-macos --example web_page
```

The real window loaded HTML/CSS/JavaScript, its button updated the document,
a foreign navigation was rejected, and close reported zero retained windows.
This preview currently exercises local DOM actions. The Runtime bridge described below can be attached before loading;
the external screen has been qualified in an isolated profile.

## Runtime bridge in the web host

Before the first pump, call `window.attach_surface(factory)`. The factory receives
the real native window ID/origin and must return a surface with that exact binding,
a declared capability catalog and mandatory deployment authorization. The host
checks the main-frame document URL and schema before accepting messages; page JSON
never selects the caller or principal. Attached documents cannot navigate/reload
into a new generation implicitly. Reconnect creates a fresh window and surface.

Native Promise replies are retained in a bounded queue: the surface operation
budget, two reserved close/cancel slots and 16 MiB request bytes. The explicit
deployment pump polls once per tick. Authorization and backend execution must be
nonblocking, or the deployment must provide its own bounded blocking boundary.
The host starts no executor or worker threads. Close rejects pending promises,
cancels the surface and frees its subscriptions; effects are not rolled back.

The TypeScript client exports `connectUiWeb`, using the host's public bootstrap
metadata and native Promise transport. It preserves typed operations, controlled
errors, deadlines, backpressure and idempotency keys. Request IDs use the standard
cryptographic random-byte API without a third-party library.

```sh
cargo run -p appcore-ui --features web-macos --example web_bridge
```

This explicitly labeled SDK fixture renders the real TypeScript client in the
system web component. Query, command, subscription and same-key replay were
verified: one effect, one Runtime event, one UI notification. Forged authority
and another surface ID were rejected. Closing reported zero pending requests
and retained windows. Fixture authorization is not a production login system.
An independent external consumer reused its existing sync-status screen and CSS.
The real macOS window displayed synthetic state, performed two queries through
this bridge, and exited 0 with zero pending requests and retained windows.
This verifies a source consumer, not production authentication/data or a full
application migration. Workspace qualification is tracked in the implementation plan.

## Interactive examples and native functions

The native profile now includes editable text, checkbox, dropdown, slider and determinate progress, besides text/buttons. `elements::{text,button,input,toggle,select,slider,progress}` are pure Rust constructors; validate the enclosing page. `UiNativeWindow::{set_label,set_value,set_enabled,value}` update/read existing controls with typed validation and page/label budgets, without changing action bindings or granting authorization. Dropdowns admit 1–64 unique nonempty labels; user input is bounded when read, not a guarantee about all memory inside system editing controls.

The gallery uses Editing → Review → Applied, revision checks, locked controls and scripted typed commands. No AI model is connected. The runner has deterministic physics, collision/score/restart/pause. The 3D example uses direct system WebGL, perspective/depth and six Lorenz trajectories, RK4 integration, 24,576 bounded point samples, orbit/zoom, trail isolation, pause and single-step controls; it is not a Rust-native 3D engine.

```sh
cargo run -p appcore-ui --features native-macos --example native_gallery
cargo run -p appcore-ui --features native-macos --example native_counter
cargo run -p appcore-ui --features web-macos --example web_runner
cargo run -p appcore-ui --features web-macos --example web_3d
```

All four examples were rendered and interacted with in real macOS windows; closing each exited 0 with zero retained windows. The component/AI proposal is a roadmap, not an implemented tool protocol.

## Input responsiveness

The pump services pending run-loop work even under native input load, and bridge work before/after native dispatch. Native draining stops when the activation queue is full; consume `pending_activations` with `next_activation` after pumping. System tracking/callbacks can exceed the cooperative drain budget. The web view accepts the first physical click when activating an inactive window; this changes focus behavior, not authorization. Physical click checks received ten of ten actions in native and web examples, plus one click when switching windows.

```toml
[dependencies]
appcore-ui = { version = "=1.0.0-rc.1", features = ["web-macos"] }
```

[crates.io](https://crates.io/crates/appcore-ui/0.1.0-beta.2) · [API](https://docs.rs/appcore-ui/0.1.0-beta.2/appcore_ui/)

## Release candidate — 2026-10-14

`0.1.0-beta.2`: dependency metadata alignment.

The source 3D showcase now also includes a deterministic 12,000-star galaxy, a procedural terrain and a vertex-shader wave surface (8,192 triangles each). Scene selection, pause/step/reset and orbit/zoom share the bounded WebGL renderer. These are local example capabilities, not native Rust GPU compute, physical fluids or a 4K performance claim.

The source gallery starts in Fractal city: 391 boxes (4,692 triangles), three recursive tower levels, skybridges, directional face lighting and fog. Click the canvas to focus; WASD flies, Q/E changes altitude, Shift boosts, drag looks around, R returns home and Esc releases focus. Flight has bounded coordinates and no solid collision. Keys clear on focus/visibility/context loss. This finite recursive example runs in WebGL; native Rust 3D remains planned.

Shader recovery: shared uniforms and varyings use matching explicit precision across WebGL stages; compilation/link errors now include a bounded driver diagnostic. A regression checks the shader interface.

Live macOS qualification after the shader fix: city rendering, W forward movement, E altitude, R home and scene switching to animated waves were exercised in the real system web window. This does not qualify every scene or device-loss recovery.

Source native presentation prototype: `gpu-macos` optionally attaches a borrowed Metal surface to a native window. One creation attempt per physical window, main-thread renderer, two submissions maximum, completion callbacks, explicit zero-size suspension and resize backpressure. `native_gpu` pumps startup/render/shutdown itself and presents an animated clear. No interactive scene/compute yet. Surface loss requires explicit reconfiguration; device loss closes this generation and recreation is deployment-owned. Platform acquisition/configure can still stall; no strict latency guarantee is asserted.

```sh
cargo run -p appcore-ui --features gpu-macos --example native_gpu
```

Presentation qualification: bounded RAII submission permits passed failure/drop and 16-thread contention tests. Real macOS presentation received 10 physical clicks plus 10 native-control activations (counter 20), survived zoom and restore, and closed with frames=7622, clicks=20, retained_windows=0. Minimized windows now report zero graphics dimensions to suspend submissions; minimize/restore, injected loss and recreation still require qualification.

Recovery qualification passed on real Metal: six minimize/restore cycles, two injected surface-state losses, one explicit device destruction with loss callback, and recreation on a fresh physical window. Generations presented 202 and 213 frames, both with pending_before_drop=0 and retained_windows=0. This is deliberate device destruction, not physical GPU reset or spontaneous compositor loss. Fault hooks require the opt-in `qualification` / `gpu-qualification` features; default features remain unchanged.

```sh
cargo run -p appcore-ui --features gpu-qualification --example native_gpu_recovery
```

Native Lorenz is now wired to presentation: GPU RK4 and instanced particle quads, perspective camera, depth, six colors and bounded 24,576 samples. Native controls pause/resume, step, reset, orbit/tilt, zoom and isolate. Real macOS check observed the full attractor, pause, isolation, camera movement, reset and a single step increasing samples 240→288 (8×6). Resume and shutdown passed: frames=6747, actions=9, retained_windows=0. No per-frame particle readback; frame admission covers compute plus drawing. Warmup progresses at eight steps per presented frame. ECS/world/virtualization/massive physics are still future stages.

```sh
cargo run -p appcore-ui --features gpu-macos --example native_lorenz
```

`GpuMeshBatch` supports shared opaque static geometry, poses/colors, conservative GPU frustum selection and draw_indexed_indirect. `GpuMeshScene` composes up to 64 fixed batches and 65536 reserved instances: frustum prepass → fresh max-depth Hi-Z → conservative queries → compacted indices → final indirect draws. Prepass/final buffers and depth targets are distinct; the pyramid is rebuilt once per frame, with far padding for odd viewports. Camera/near-plane and viewport crossings remain visible. Groups match descriptor order and mesh identity; empty groups reset counts. Managed reservation includes both depth targets, all buffers and padded mips; standalone scene excludes caller-owned color. Native presentation admits that color too. Initial implementation duplicates shared geometry between prepass/final batches, counted in the budget.

Real Metal pixel tests verify depth/color, masked suppression/reappearance, multi-mesh composition, invalid groups, budget rejection and empty-frame clearing. Native multi-mesh qualification displayed 256 cubes + 64 pyramids, delivered ten consecutive physical orbit clicks, and exercised pause, zoom, orbit, reset, resume and zoom/restore resizing. Shutdown: frames=3102, actions=15, retained_windows=0. Earlier surface qualification covered six minimize/restore cycles, two injected surface losses, deliberate device destruction/loss callback and explicit recreation (202/213 frames, zero pending/retained windows). This is not a physical GPU reset. Fault hooks are opt-in qualification/gpu-qualification.

```sh
cargo run -p appcore-ui --features gpu-macos --example native_meshes
```

LorenzParameters exposes bounded sigma (1–20), rho (1–50), beta (0.5–6), rejecting nonfinite values. reset_with_parameters/configure_lorenz restart trajectories and trail history after completion; reset preserves coefficients. The fixed timestep remains 0.005; bounds do not guarantee universal numerical stability. Metal tests compare custom coefficients with an f64 reference and verify invalid input leaves state untouched. The native Params button cycles three bounded presets. Real-window qualification exercised 0→1→2→0, reset history in pause (zero samples), a single GPU step (48 samples), resume and close.

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

Native shutdown now exposes abort_close for an error or an expired drain deadline. It closes and destroys that device generation without a GPU wait or synthetic frame-completion updates; repeated calls are harmless. Drop also destroys the device before the borrowed surface owner can be released, covering early error returns. Prefer begin_close/finish_close for normal draining. Device destruction is a backend call, not a hard wall-clock guarantee; abort does not prove pending frames executed or reached display. After abort, finish_close returns RendererClosed even if begin_close is called again; aborted work cannot be reported as a normal successful drain.

Lorenz resize admission now counts the retained old Depth32Float texture together with the new color/depth targets and fixed simulation/camera buffers before surface reconfiguration. Equal dimensions reuse depth storage. A peak-budget rejection keeps the previous scene and configuration usable; zero dimensions still suspend. Pure tests cover grow/shrink, exact byte limits and unchanged dimensions. Native recovery qualification additionally rejects a grow whose final allocation fits but replacement peak does not, then presents the preserved Lorenz scene.

The opt-in gpu-qualification feature adds UiNativeWindow::qualification_content_size(width, height), in logical points. It changes the real AppKit content size within the page’s original minimum/maximum dimensions and 4194304-point area ceiling, rejecting invalid sizes before mutation and closed windows as unavailable. It does not resize the renderer: the owner pumps, reads graphics_size in physical pixels and explicitly reconfigures after GPU completion. native_gpu_recovery now exercises nine real Lorenz window resizes, three Lorenz minimize/restore cycles, paused-history preservation, reset and a single step, with 21 submitted/completed frames and zero retained windows. Invalid size requests preserve physical size. This automated lifecycle qualification does not prove visual quality or physical input latency.


The native_lorenz, native_meshes and native_lod examples include a readable performance HUD: successful presents/second, mean/p95 inter-present interval (128 samples), CPU present-call time, process resident RAM (RSS), managed GPU payload, physical resolution, adapter and submitted/completed frame counters. Values refresh twice per second. CPU submit is not GPU execution time; GPU payload is budget accounting, excluding driver/compositor/extra swapchain allocations, not measured VRAM. These metrics do not prove display scanout or 4K60.

Click below the HUD to activate camera controls: drag to orbit, W/S or wheel to zoom, A/D or left/right arrows to orbit, up/down arrows to tilt. Escape, Tab, control clicks and window focus loss release navigation and clear held keys. OS shortcuts pass through. This is bounded orbit/zoom, not free flight or object manipulation. Manual navigation stops automatic mesh orbit; Resume restarts it. Lorenz simulation remains independent. The viewers stay open until closed; optionally set E3DN_DEMO_SECONDS=30 (1–600) for a bounded run.

With gpu-macos, enable_scene_input(overlay_height) opts a graphics window into fixed-state navigation; take_scene_input(elapsed) drains deltas, caps elapsed at 50 ms and reports focus without exposing raw key text. native_process_resident_bytes reads current process RSS through the system Mach API; failures remain explicit. Label backgrounds over graphics are opaque for readability.

Native visual/input qualification on Apple M1/Metal at 1024×768 verified the HUD and camera changes through mouse drag and held keys in all three examples; LOD additionally exercised wheel, Escape and Reset. Observed values were approximately 50 presents/s, 20 ms inter-present interval, RSS 53–106 MiB and managed payload 6.4–14.5 MiB, specific to this local run. LOD drained 8972/8972 frames, meshes 7115/7115, and Lorenz closed after 780 frames; all reported retained_windows=0. This supersedes the earlier locked-session input blocker for these interactions; it does not qualify every recovery scenario or 4K60.
