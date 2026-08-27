# appcore-api

Les observations de `appcore-sync 1.0.2-rc` sont faillibles. Status privé
et diagnostics exposent `sync_log_len: null` avec
`sync_log_observation_ok: false` lorsque le provider actif ne peut pas être lu,
sans annoncer un état ancien.

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

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
directe, le HTTP et le peer RPC libèrent le mutex d'état du host avant
l'exécution. Les queries indépendantes s'exécutent en parallèle ; un appel
tardif à `register_query` échoue avec `router_frozen`.

Dans `1.0.2-rc`,
`ReloadableRuntimeHttpHost` fournit une transaction explicite de génération de
routing. `prepare` accepte seulement une génération plus récente sur la même
adresse liée. `reload` exécute `/v1/health` avant activation, commute
atomiquement le routing des nouvelles requêtes, vérifie encore la santé puis
draine l'ancien in-flight. Si la santé après commutation ou le drain échoue,
l'ancienne génération est restaurée et la génération défaillante ferme son
admission avant nettoyage. Une requête admise ne change jamais de router. Les
délais sont positifs et plafonnés à 60 secondes; les snapshots ne contiennent
aucune identité de requête.

Les changements d'adresse restent hors de cette primitive à listener stable.
La composition root doit préparer un second listener et le coordonner avec le
Supervisor existant. Il n'existe ni watcher automatique du manifest V1 ni
fallback.
Pour valider le bind avant le démarrage sur l'adresse stable, la composition
root peut transférer un listener TCP déjà lié via
`run_on_listener_until_shutdown`.

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
