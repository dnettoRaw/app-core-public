# appcore-provider-vercel-neon

Release candidate **`appcore-provider-vercel-neon 1.0.4-rc.3`**. Select this RC explicitly; it does not replace earlier stable releases or certify a deployment. Internal AppCore dependencies, where present, are pinned to each dependency’s independent RC.

Lease acquisition/renewal and release use one HTTP attempt, regardless of
retry configuration. V1 supplies no remote deduplication key; a lost response
may follow an applied mutation. Timeout or transient HTTP failure must not be
treated as proof that the lease was unchanged. Reconcile authoritative lease
state and fencing before leader-dependent writes; do not blindly replay the
operation. Discovery, registration and heartbeat retain their configured retry
policy; their remote semantics still require deployment conformance testing.

Factory retry settings are validated before secret resolution: attempts 1–16,
timeout and backoff values 1–30,000 ms, with initial backoff no greater than
maximum backoff. The conservative sum `timeout × attempts + max_backoff ×
(attempts − 1)` must not exceed 120,000 ms. Invalid configuration is rejected,
not clamped. This admission budget is not an enforced wall-clock deadline.

[Minimal example](examples/basic.en.md) |
[Intermediate example](examples/intermediate.en.md)

**Responsibility:** isolated official factory for the Vercel API control-plane
adapter backed by externally operated Neon coordination.

**Internal dependencies:** contracts, control plane and provider.

**Primary API:** `VERCEL_NEON_PROVIDER_ID`, `AUTH_TOKEN_SECRET`, shared
control-plane client type and `VercelNeonControlPlaneFactory`.

Runtime nodes receive only the Vercel endpoint and an auth-token secret
reference. Neon credentials, schema operations, backup and retention stay in
the external service.

**Maturity:** supported RC adapter; production certification includes the
separately operated backend.

## Release candidate — 2026-10-14

`1.0.4-rc.2`: dependency metadata alignment.
