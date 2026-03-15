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

Both `appcore-bin` and `appcore-auth-server` use the bounded `appcore-args`
input boundary. Their help and completion candidates come from the same
validated command specification.

The final manifest capability descriptors are composed once through
`appcore-capabilities`. Direct facade, application HTTP and peer RPC dispatch
all use that owner for mode, idempotency, write-mode and leadership
enforcement.

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
