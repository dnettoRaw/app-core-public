# appcore-provider-vercel-neon

**Responsabilidade:** factory oficial isolada do adapter Vercel API com
coordenação Neon operada externamente.

**Dependências internas:** contracts, control plane e provider.

**API principal:** `VERCEL_NEON_PROVIDER_ID`, `AUTH_TOKEN_SECRET`, tipo shared
do client e `VercelNeonControlPlaneFactory`.

Nodes recebem somente endpoint Vercel e referência do auth token. Credenciais,
schema, backup e retention Neon ficam no serviço externo.

**Maturidade:** adapter RC suportado; certificação inclui o backend separado.
