# appcore-provider-vercel-neon

**Responsabilité :** factory officielle isolée de l'adapter API Vercel avec
coordination Neon opérée extérieurement.

**Dépendances internes :** contracts, control plane et provider.

**API principale :** `VERCEL_NEON_PROVIDER_ID`, `AUTH_TOKEN_SECRET`, type
partagé du client et `VercelNeonControlPlaneFactory`.

Les nodes reçoivent seulement endpoint Vercel et référence auth token. Les
credentials, schémas, backup et retention Neon restent dans le service externe.

**Maturité :** adapter RC supporté; certification incluant le backend séparé.
