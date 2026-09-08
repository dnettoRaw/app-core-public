# Migrating from appcore-bin

Replace the application dependency and imports; do not reproduce the old host.
The final registry release of `appcore-bin` is only a retirement notice. New
code must depend directly on `appcore-sdk`.

1. Depend on `appcore-sdk` and enable only the capabilities used by the app.
2. Import `Application` and registry contracts from `appcore_sdk`.
3. Keep `application.toml`, `deployment.toml` and business code unchanged.
4. Use `App::prepare` to validate and collect business registrations.
5. Let the deployment executable resolve providers, call
   `prepare_with_deployment`, start workers and own shutdown.

There is no compatibility alias for `appcore_bin`, no Runtime CLI in the SDK
and no implicit provider selection. Removed host operations must fail at the
deployment boundary instead of being inferred by the facade.
