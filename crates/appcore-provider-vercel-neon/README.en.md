# appcore-provider-vercel-neon

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

[Português](README.pt.md) | [Français](README.fr.md)

`appcore-provider-vercel-neon` is the isolated official adapter that lets an
AppCore deployment use a Vercel-hosted control-plane API backed by an externally
operated Neon service. Runtime nodes call HTTPS; they never connect to Neon.

## What the crate provides

- `VercelNeonControlPlaneFactory`, registered under
  `VERCEL_NEON_PROVIDER_ID`;
- `SharedControlPlaneProvider`, the provider type returned to the composition
  root;
- `AUTH_TOKEN_SECRET`, the exact deployment secret slot expected by the
  factory;
- validation of the endpoint, settings, and resolved bearer token before the
  control-plane client becomes available.

A deployment selects the provider explicitly and supplies an HTTPS endpoint
plus a secret reference:

```rust
use appcore_contracts::{ProviderConfig, ProviderId, SecretRef};
use appcore_provider_vercel_neon::{
    AUTH_TOKEN_SECRET, VERCEL_NEON_PROVIDER_ID,
};

let config = ProviderConfig::new(ProviderId::new(VERCEL_NEON_PROVIDER_ID)?)
    .with_endpoint("https://control.example.com")?
    .with_secret_ref(
        AUTH_TOKEN_SECRET,
        SecretRef::new("env:APPCORE_CONTROL_TOKEN")?,
    )?
    .with_setting("timeout_ms", "5000")?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

## Trust boundary

The Runtime Deployment Manifest contains only the Vercel endpoint and a token
reference. Neon connection strings, database credentials, schema migrations,
backup, and retention remain in the separately operated service. Missing
secrets, non-HTTPS endpoints, invalid settings, authentication failures, and
unavailable remote service fail explicitly; this adapter does not fall back to
another provider.

See the [basic example](wiki/examples/basic.en.md), the
[intermediate example](wiki/examples/intermediate.en.md), and the
[crate guide](wiki/guide.en.md). Run:

```bash
cargo test -p appcore-provider-vercel-neon
```

## Stable documentation

Stable ID: **ACR-020**. See the
[supplemental architecture and integration guide](https://wiki.appcore.dnettoraw.com/crates/id/acr-020). This permanent ID
remains valid if the wiki page moves.
