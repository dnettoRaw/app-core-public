# appcore-provider-vercel-neon

Acquisition/renouvellement et libération de lease utilisent un seul essai HTTP,
quelle que soit la configuration des retries. V1 ne fournit pas de clé de
déduplication distante ; la réponse peut se perdre après application. Timeout
ou erreur HTTP transitoire ne prouvent pas que le lease est inchangé. Réconciliez
l'état autoritatif et le fencing avant les écritures dépendant du leadership,
sans rejouer aveuglément. Discovery, enregistrement et heartbeat conservent
leurs retries configurés ; la sémantique distante exige des tests de conformité.

La factory valide les retries avant de résoudre les secrets : 1–16 tentatives,
timeout et backoffs de 1 à 30 000 ms, avec un backoff initial inférieur ou égal
au maximum. La somme conservatrice `timeout × tentatives + max_backoff ×
(tentatives − 1)` ne doit pas dépasser 120 000 ms. Une configuration invalide
est rejetée, pas ajustée. Ce budget n'est pas un délai réel imposé à l'opération.

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** factory officielle isolée de l'adapter API Vercel avec
coordination Neon opérée extérieurement.

**Dépendances internes :** contracts, control plane et provider.

**API principale :** `VERCEL_NEON_PROVIDER_ID`, `AUTH_TOKEN_SECRET`, type
partagé du client et `VercelNeonControlPlaneFactory`.

Les nodes reçoivent seulement endpoint Vercel et référence auth token. Les
credentials, schémas, backup et retention Neon restent dans le service externe.

**Maturité :** adapter RC supporté; certification incluant le backend séparé.
