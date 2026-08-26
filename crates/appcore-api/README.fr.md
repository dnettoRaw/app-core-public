# appcore-api

**Responsabilité :** host HTTP command/query/status et DTOs de transport.

**Dépendances internes :** `appcore-core`, `appcore-security` et
`appcore-supervisor`.

**API principale :** `CommandRequest`/`CommandResponse`,
`QueryRequest`/`QueryResponse`, erreurs de validation, `CommandEndpoint`,
`QueryEndpoint`, `ApiRouter`, `ApiRequest`/`ApiResponse`, `RuntimeHttpHost`,
`HttpApiConfig`, statut statique, policy capability pour commands et queries
applicatives, vérification token et vue du sync log.

À utiliser pour les routes Runtime et queries applicatives enregistrées. Ne pas
ajouter resources REST produit ou schémas métier. Le nouveau host l'utilise via
`appcore-bin`.

Les queries applicatives sont autorisées par la policy capability composée
avant le router. Les queries de statut Runtime restent hors du catalogue
applicatif.

Les hosts Runtime gèlent l'enregistrement des queries de `ApiRouter` après le
bootstrap. Les clones du router partagent les endpoints via `Arc` ; la façade
directe, le HTTP et le peer RPC libèrent le mutex d'état du host avant d'appeler
l'endpoint, ce qui permet l'exécution concurrente de queries indépendantes.

Dans la prochaine version majeure, `SyncLogView::len` et `is_empty` sont
faillibles. Le status JSON privé retourne `sync_log_len: null` avec
`sync_log_observation_ok: false` lorsque la persistance active ne peut pas être
observée ; il ne substitue jamais un ancien compteur statique.

La limite configurée s'applique au corps HTTP complet avant la
désérialisation JSON par Axum. Les routes protégées acceptent exactement un
header bearer `Authorization` bien formé; les doublons échouent fermés.

`HttpCommandAuth::default()` exige l'authentification et échoue fermé tant
qu'aucun vérificateur de token n'est configuré. Seul
`insecure_local_for_testing()` désactive explicitement l'authentification
command/query pour des tests locaux contrôlés. `/v1/health` reste public par
contrat. Les refus d'autorisation command sont audités avec des métadonnées
normalisées, sans credentials, payload ni clé d'idempotence.

**Maturité :** surface HTTP V1 RC stricte et stable.
