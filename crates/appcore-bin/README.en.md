# appcore-bin

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

Manifest-first application facade, Runtime CLI and composition root.

New applications implement `appcore_bin::application::Application` and call
`run_application`. The host owns manifest loading, providers, lifecycle, HTTP,
security, scheduling, sync, Gateway, distributed services, updates and
shutdown.

Application code should not import private host modules.

## Opt-in Windows DPAPI keyring (1.5 alpha)

On Windows, select secret provider `windows-dpapi-user-v1`, set its `root`, and
set `runtime_security = "provider:active"`. Initialize and rotate the same
explicit provider with `appcore-bin security secret keyring-init|keyring-rotate
--keyring PATH --keyring-provider windows-dpapi-user-v1`. The current-user,
current-machine scope never falls back to the existing file keyring. AC-009 real
Windows certification is still pending; stable 1.0 behavior is unchanged.

## Opt-in AI alpha

The `ai-alpha` feature attaches an already configured `appcore_ai::AiRuntime`
to the existing Supervisor without changing frozen V1 manifests:

```rust
let component = Arc::new(AppCoreAiComponent::new(Arc::new(ai_runtime), false)?);
let ai = component.facade();
let business = MyApplication::new(ai);
ManifestApplicationHost::load("application.toml", "deployment.toml", &business)?
    .with_ai(component)
    .run()?;
```

`required = true` fails startup when no model/backend is usable; `false`
starts degraded. Shutdown stops admission, cancels active requests and waits
for the bounded Supervisor deadline. Exposing `appcore.ai.resolve` through
`appcore-capabilities` requires an application-owned `AiCapabilityCodec`; the
Rust request types are not an implicit wire format. Declarative model/provider
selection requires a future versioned post-1.0 manifest contract.

Both `appcore-bin` and `appcore-auth-server` use the bounded `appcore-args`
input boundary. Their help and completion candidates come from the same
validated command specification.

The final manifest capability descriptors are composed once through
`appcore-capabilities`. Direct facade, application HTTP and peer RPC dispatch
all use that owner for mode, idempotency, write-mode and leadership
enforcement.

Direct facade, application HTTP and peer RPC command handlers execute without
holding the shared host mutex. Independent commands can progress concurrently;
idempotency reservation and finalization remain serialized per store. Host
shutdown stops new admission, drains admitted commands for up to 30 seconds,
then completes the lifecycle. Tests may select a shorter bound with
`ManifestApplicationHost::shutdown_with_timeout`.
Application query registration is frozen after bootstrap; direct, HTTP and peer
RPC queries clone the immutable router and execute without the host mutex.

In the 1.5 candidate, the selected HTTP managed service uses one
`ReloadableRuntimeHttpHost` generation under the existing Supervisor. This
does not enable manifest polling or change stable routes. It establishes the
same-listener prepare/switch/drain/rollback boundary; address-changing listener
generations still require explicit composition support.

When `deployment.toml` selects `[adapters.gateway]` with provider
`appcore-gateway`, bootstrap validates the owner configuration, adds and
authorizes `runtime.gateway` in that catalog, reuses Runtime security and
registers the instance with the Supervisor. Bind or configuration failure
aborts startup. `ApplicationServiceReport` exposes Gateway started/state/bind
fields without credentials. The host supplies a durable process-safe replay
store; cluster mode requires absolute `paths.gateway_replay` on a shared writable
volume. Shutdown force-closes incomplete connections before its deadline and
joins the listener and owned runtime thread.

```bash
appcore-bin completions zsh
appcore-auth-server completions powershell
```

```bash
cargo test -p appcore-bin
```
