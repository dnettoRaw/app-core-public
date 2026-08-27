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

Command handlers reached through the direct facade, application HTTP or peer
RPC execute without retaining the shared host mutex. Independent commands can
progress concurrently; idempotency reservation and finalization remain
serialized per store. `shutdown()` closes admission and drains admitted
commands for at most 30 seconds. `shutdown_with_timeout` exposes a smaller
bounded deadline for tests and embedded hosts.
Application query registration is frozen after bootstrap; direct, HTTP and peer
RPC queries clone the immutable router and execute without the host mutex.

The 1.5 candidate composition path owns one `ReloadableRuntimeHttpHost` generation as the
existing `http` managed service. There is no second Supervisor or detached
reload worker. This integration keeps stable routing unchanged and prepares the
same-listener switch/drain/rollback boundary; it does not poll V1 manifests or
silently bind another address.

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

## Windows DPAPI secret provider (1.5 alpha)

On Windows, deployment composition accepts `windows-dpapi-user-v1` with a
non-empty `settings.root` and `runtime_security = "provider:active"`. Keyring
CLI operations must repeat `--keyring-provider windows-dpapi-user-v1`; omitting
it deliberately selects the unchanged `file-keyring-v1` behavior. The provider
is unavailable on other platforms and format or decryption mismatch fails
closed. AC-009 remains uncertified until the real Windows matrix passes.

## Opt-in AI alpha

Enable `appcore-bin/ai-alpha`, build an `appcore_ai::AiRuntime` with explicit
limits, admission, model registry and backends, then wrap it in
`AppCoreAiComponent`. Inject `component.facade()` into business code before
loading the host and finish composition with
`ManifestApplicationHost::with_ai(component)`. The existing Supervisor owns
startup, required/optional health, cancellation and bounded shutdown.

This feature is programmatic because V1 manifests are frozen. It does not
infer providers, download models or define a wire payload. Registering the
local `appcore.ai.resolve` handler requires an application-owned bounded
`AiCapabilityCodec`. See the runnable `appcore-ai` lightweight,
OpenAI-compatible and Candle examples for runtime construction.

**Maturity:** stable manifest-first RC facade; the AI integration is a separate
`0.1.0-alpha` opt-in and composition internals remain implementation details.
