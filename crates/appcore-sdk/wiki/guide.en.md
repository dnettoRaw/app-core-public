# AppCore SDK guide

The `runtime` benchmark measures default construction, empty preparation and
preparation of 32 Core hooks: commands, events, states, decision declarations,
executable decision nodes and handlers. Run `cargo bench -p appcore-sdk --bench runtime`.
`APPCORE_BENCH_CASE` selects `construct_defaults`, `prepare_empty` or
`prepare_32_core_hooks`. These measure facade construction, validation and
teardown, not HTTP startup or cluster operation. Queries and tasks are
feature-gated and are not part of this Core case. `--all-features` adds
`prepare_32_full_hooks`, which also prepares 32 frozen queries and 32 bounded
task definitions. It does not start any optional backend.

`appcore-sdk` is the application-facing facade. Start with `App` and `run`; add
explicit manifests when local identity/version or deployment policy needs them.

1. Add `appcore-sdk` to the application dependencies.
2. Call `appcore_sdk::run` with a stable application name.
3. Use the supplied `App` only for local logging and validated manifest access.
4. Implement `Application` when commands and events are needed, then call
   `App::prepare` with the explicit node identity.
5. Add `api` for HTTP queries, `scheduler` for scheduled tasks, and
   `deployment` only when the application consumes provider-resolved bindings.
6. Deploy the application with an explicit Runtime process.

The SDK does not start a listener, write data, resolve secrets, or enable a
provider implicitly. Its default manifests are canonical V1 standalone values,
not a second configuration language.

`App::prepare` is the lifecycle bridge before hosting. It runs the registration
hooks once, builds immutable Core registries, freezes the query router and
collects bounded scheduler tasks. A deployment consumes those values; it still
owns providers, workers, cancellation and shutdown. With the `deployment`
feature, the deployment calls `Application::configure` only after it has
resolved and validated installation bindings.

## Defaults and explicit manifests

A minimal local program needs one direct AppCore dependency, `appcore-sdk`,
and no manifest files. `App::new` still constructs validated
`ApplicationManifestV1` and `DeploymentManifestV1` values. The standalone
deployment names `file` storage and `http` transports but does not start them.

1. Construct `App` with the application's stable identity.
2. Optionally call `App::application_manifest` to replace the whole application
   contract, not merge selected fields into defaults.
3. Optionally call `App::deployment_manifest` to replace the whole installation
   policy. Explicit provider/network values do not inherit SDK defaults.
4. Both manifests must match the `App` identity. Invalid contracts or mismatched
   identities return an error; defaults never hide the failure.
5. Inspect `effective_application_manifest()` and
   `effective_deployment_manifest()`, then call `run` or `prepare`.

Explicit manifests are also useful for local version, service and installation
identity changes; they do not require hosted services. Deployment integration
owns reading files and resolving providers. The SDK does not discover or load
manifest files automatically.

| Need | SDK surface |
|---|---|
| Local callback and logging | `run`, `App`, `AppResult`, `App::logger` |
| Registered behavior | `Application`, explicit `NodeId`, `PreparedApplication` |
| Queries / tasks | `api` / `scheduler` features and prepared registries |
| Storage / replication | `storage` / `sync` namespaces and features |
| AI / documents | `ai` / `filemaker` namespaces and features |
| Resolved installation bindings | `deployment` feature; `prepare_with_deployment` |
| Process lifecycle | Application deployment, not an SDK host |

Enabling a feature exposes APIs, not running services. Process identity,
providers, cancellation and shutdown remain explicit deployment responsibilities.

See [basic](examples/basic.en.md) and [intermediate](examples/intermediate.en.md).

The executable catalog mirrors this growth path: `01-basics`, `02-manifests`,
`03-application`, `04-storage`, `05-sync`, `06-ai` and `07-filemaker`.
`08-logging` and `09-api` complete the application-facing catalog.

The default SDK has no API, provider or scheduler dependency. `storage`,
`sync`, `ai` and `filemaker` are opt-in as well; use `full` only for a
development consumer that intentionally needs every SDK capability.

Logging is configured once with `App::logging(LoggerConfig)`. Select terminal,
file, both, disabled or crash-only output. The file name, size, nearby
rotations and optional bounded `YYYY/MM` archive remain explicit. Crash-only
mode writes its bounded in-memory diagnostics through `App::dump_crash_log`.
