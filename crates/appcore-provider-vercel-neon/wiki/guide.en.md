# appcore-provider-vercel-neon

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
