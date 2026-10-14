# appcore-ui

Independent UI module, public beta `0.1.0-beta.2`.

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
appcore-ui = { version = "=0.1.0-beta.2", features = ["web-macos"] }
```

[crates.io](https://crates.io/crates/appcore-ui/0.1.0-beta.2) · [API](https://docs.rs/appcore-ui/0.1.0-beta.2/appcore_ui/)

## Release candidate — 2026-10-14

`0.1.0-beta.2`: dependency metadata alignment.
