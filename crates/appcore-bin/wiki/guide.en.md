# appcore-bin

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** manifest-first application facade, Runtime CLI and
composition root.

**Internal dependencies:** all Runtime service/composition crates.

**Primary application API:** `Application`, `run_application`,
`ManifestApplicationHost`, `ApplicationServiceReport`, `DeploymentContext`,
resolved volume/environment values and `ApplicationTaskRegistry`.

**Host API:** typed bootstrap/configuration errors and results, CLI parsing and
commands, local paths/lifecycle, server entry points, build information and
optional auth-server grant tooling.

Both binaries parse bounded UTF-8 input through `appcore-args`. Generated help,
validation and dynamic Bash, Zsh, Fish and PowerShell completion share one
declarative command specification; command execution remains in this crate.

The final distributed manifest feeds one `appcore-capabilities` catalog during
bootstrap. Direct facade, application HTTP and peer RPC dispatch use that same
owner for declaration, mode, idempotency, operational-write and leadership
enforcement. Runtime-owned status queries remain explicit host behavior.

Selecting `[adapters.gateway]` with provider `appcore-gateway` is the
declarative Gateway activation boundary. Bootstrap parses the configuration
through the owner crate, adds and authorizes `runtime.gateway` in the shared
catalog, reuses Runtime security and registers the service with the Supervisor.
Configuration or bind failure aborts startup; omission creates no Gateway
listener or task. `ApplicationServiceReport` exposes safe started, state and
bind fields. The host supplies a process-safe replay store; cluster requires
absolute `paths.gateway_replay` on a shared writable volume. Shutdown force-closes
incomplete connections before joining all Gateway-owned work.

This is the recommended dependency for new applications. The crate owns
manifest loading, provider composition, lifecycle, HTTP, sync, peer RPC,
control plane, Gateway, scheduling, supervision, updates and shutdown.

Application code must use the public `application` module and avoid private host
internals.

**Maturity:** stable manifest-first RC facade; composition internals remain
implementation details.
