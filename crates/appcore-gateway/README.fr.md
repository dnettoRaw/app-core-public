# appcore-gateway

**Responsabilité :** relay WebSocket isolé par tenant pour les connexions
Gateway entre clients externes et workers AppCore.

**Dépendances internes :** contracts, types, security, distributed
contracts et peer RPC.

**API principale :** `GatewayConfig`, `GatewayState`, état par tenant, registry
et resolver de capability, connexions worker/client bornées,
`MeshPeerTransport`, DTOs request/response du mesh relay, pruner heartbeat et
factory du router Axum. Les contrats content-envelope opaque sont réexportés
pour router des payloads chiffrés.

> **Migration du RC actuel :** l'accès direct à
> `GatewayState::tenants` a été supprimé afin que des tenants indépendants ne
> partagent plus un verrou unique. Le code qui utilise ce champ échoue à la
> compilation et doit utiliser
> `tenant_partition`, `tenant_partition_or_insert`, `tenant_count` et
> `connection_count`. Les anciennes maps publiques des requests en attente sont
> aussi privées ; observez-les avec `pending_request_count` et laissez
> `EnvelopeRouter` gérer leur cycle de vie. Consultez
> [le guide de migration](../../release/gateway-tenant-migration.md). Aucun
> alias historique ni map miroir n'est fourni.

## Composition dans le Runtime

`appcore-bin` est le composition root. Un deploiement active cette crate avec
la map d'adapters existante :

```toml
[adapters.gateway]
provider_id = "appcore-gateway"
settings = { bind_address = "127.0.0.1:8080", domain_suffix = "gateway.example.com", heartbeat_interval_ms = "30000", heartbeat_timeout_ms = "90000" }
secret_refs = {}
```

Les deployments cluster doivent aussi diriger toutes les instances Gateway
vers le meme fichier replay par chemin absolu sur un volume partage et inscriptible :

```toml
paths = { gateway_replay = "/shared/appcore/gateway-connection-jti.json" }
```

L'adapter accepte uniquement ces quatre settings. Les endpoints, references de
secret, settings inconnues et tentatives de configurer l'authentification sont
rejetes. L'authentification reste obligatoire pour les instances composees par
manifest.

Pendant le bootstrap, le host ajoute la capability owner `runtime.gateway` au
catalogue, l'autorise avec `RuntimeCapabilityPolicy`, reutilise le provider de
securite du Runtime et enregistre le Gateway comme service critique du
Supervisor. Une configuration invalide ou un bind impossible arrete le
startup; sans `adapters.gateway`, aucune task ni aucun port n'est cree.

Le gateway résout le tenant depuis le suffixe de domaine défini par le
deployment ou depuis un paramètre de query réservé aux tests locaux, authentifie
les connexions lorsque configuré, route les enveloppes Peer RPC et les requests
HTTP Peer RPC via mesh relay uniquement dans la partition du tenant et retire
les workers stale avec des files de sortie bornées.

Les upgrades authentifies acceptent les credentials uniquement dans le header
`Authorization` ; les credentials en query sont rejetes. Les tokens worker
utilisent `worker_connection_hash` pour lier tenant, cluster, installation,
Core et capabilities. Les tokens client utilisent `client_connection_hash`
pour lier tenant, cluster et device. Ce sont des tokens `peer` a usage unique,
avec `jti`, request hash et une duree maximale de 60 secondes ; le socket expire
avec le token.

Le mesh relay valide le schema V1, les metadonnees de routage Peer RPC internes,
le digest du body et le hash signe avant forwarding. Le payload applicatif
reste opaque. Frames et messages sont limites a 4 Mio ; les limites tenant,
connexion, capability, request en attente, timeout, queue et routage concurrent
echouent fermees. Le heartbeat exige le JSON exact et une reponse worker n'est
acceptee que depuis la generation de connexion selectionnee.

`mesh-relay` est un peer transport pour les Cores qui gardent des connexions
Gateway sortantes au lieu d'exposer des ports locaux ou IPs stables. Ce n'est
pas un systeme de consensus, un terminateur TLS public ni un gestionnaire de
secrets de production. La federation edge relay et les transports alternatifs
ne doivent pas affaiblir l'authentification, expiry, nonce ou replay protection
de Peer RPC.

Le RC actuel inclut le contrat `GatewayRegistryProvider` et son implementation
`RedisGatewayRegistryProvider`. Elle exige TLS hors loopback, un credential
resolu separement, des limites timeout/concurrency et des scripts atomiques
dans le hash slot du tenant. Une mutation au resultat ambigu n'est jamais
rejouee; le caller entre en isolation et appelle explicitement `reconnect`.
`GatewayHaLifecycle` ferme deja admission HTTP/WebSocket, dispatch et
completion hors de `Healthy`, sans modifier le mode single-instance.
`GatewayHaCoordinator` acquiert et renouvelle maintenant l'ensemble complet et
borne des leases tenant avant `Healthy`; un round partiel, stale ou incertain
efface les leases locaux et entre en `Isolated`. Chaque round est serialise,
limite a 64 operations concurrentes et cinq secondes au total.
`GatewayRuntime::with_ha_coordinator` possede la task recovery/shutdown, rejoue
le snapshot complet et borne workers/sessions avant `Healthy`, enregistre les
nouveaux sockets avant admission locale et supprime les records exacts au
disconnect ou prune heartbeat. Le chemin local claim maintenant les epochs
origin/target et la generation worker avant dispatch, complete avant de
retourner un succes et annule sur panne de queue, timeout ou shutdown; un future
abandonne expire sous 30 secondes. Le provider peut verifier le claim live exact
sans le consommer avant l'admission target. La route V2 de federation reste en attente;
le schema strict lie maintenant body, epochs, generation et credential separe
a usage unique, avec erreurs AC-021 typees. La route HTTP bornee passe un E2E
avec deux etats Gateway et complete le fence avant d'accepter la reponse; la
meme preuve passe avec Redis 7.4 et via Caddy 2.11.4 sans bypass direct de
l'origin. Le recovery apres perte de l'owner route aussi avec un epoch superieur
apres le TTL borne. AC-022 et les preuves plateforme restent obligatoires avant
le deploiement HA; aucun fallback local n'est permis.

Le host persiste les identites de connexion a usage unique avec le
`FilePeerNonceStore`, sur entre processus. Standalone utilise le storage prive
du Runtime; cluster exige `paths.gateway_replay` absolu sur un volume partage et
inscriptible et echoue ferme s'il manque ou est indisponible. Les sockets actifs
expirent avec leur credential apres 60 secondes maximum. Les embedders utilisent
un store local borne par defaut ou injectent un `PeerNonceStore` durable/partage
avec `GatewayState::with_replay_store` ou `GatewayRuntime::with_replay_store`.

`GatewayRuntime` possede listener, thread de runtime, router et pruner. `stop`
demande d'abord un shutdown graceful, puis abandonne le future serveur avant le
delai pour fermer les connexions incompletes et joindre la thread. `Orphaned`
reste une quarantaine defensive pour une panne inattendue de thread, pas le
chemin normal d'un timeout. Le snapshot n'expose jamais credentials ou tokens.

Les hashes de connexion worker et client utilisent un framing binaire
canonique V2 avec le marqueur `v2:`. Les anciens hashes sans version ne sont
pas interchangeables; émetteurs de token et consommateurs Gateway doivent être
mis à jour ensemble.

Chaque tenant conserve des index workers directs et bornés par Core ID et par
`(cluster_id, core_id)`. Les lookups de routage sont O(1) ; register, reconnect,
disconnect et prune heartbeat mettent à jour map, registre de capabilities et
index sous le même verrou tenant. `worker_index_rebuilds` et
`worker_index_inconsistencies` exposent des compteurs bornés de santé d'index.

## Sélection déterministe des workers dans `1.0.3-rc`

L'enum V1 exhaustif `SelectionPolicy` reste limité à `FirstAvailable`.
`WorkerSelectionPolicy` fournit les choix opt-in `RoundRobin`, `LeastInflight`,
`HealthWeighted` et `Affinity`, tandis que `FirstAvailable` reste le défaut.
Les consommateurs RC des variantes avancées doivent modifier le nom de l'enum ;
aucun manifeste ni contrat wire ne change. L'ordre d'identité des candidats est stable et ne dépend pas de l'itération
d'un `HashSet`. `CapabilityResolver::select` reçoit des entrées live bornées et
rejette capability absente, worker stale/déconnecté, worker épuisé et affinity
invalide avec des valeurs `WorkerSelectionError` distinctes.

Affinity ne conserve aucune map : le rendezvous hashing inclut tenant,
capability, clé bornée et identité worker. Le dispatch Peer RPC et mesh ne
réécrit pas la cible V1 signée. Il impose indépendamment au plus 64 routes
inflight par worker, avec un permit libéré sur chaque chemin terminal. Le
planning ne contourne donc pas l'admission, et la télémétrie expose des outcomes
fixes unhealthy/capacity et le pic inflight worker sans labels d'identité. Voir
[`release/gateway-worker-selection-rc.md`](../../release/gateway-worker-selection-rc.md).

## Télémétrie bornée dans `1.0.4-rc`

`GatewayMetrics::telemetry_snapshot` et `GatewayRuntime::details`
exposent p50/p95/p99 par buckets fixes pour route, attente worker, verrou tenant
et taille du payload. Ils exposent aussi inflight/pic, pic de profondeur de
queue, reconnect, retry, authentification, saturation, timeout, rejet
unhealthy/capacity, pic inflight worker, overflow et échec exporter. Au plus
128 noms de capability validés sont conservés ; les noms suivants utilisent
une série d'overflow fixe. Tenant, installation, Core, request, connexion,
credential, payload et texte d'erreur ne sont jamais des labels.

`GatewayTelemetryExporter` reçoit uniquement un snapshot possédé quand
l'opérateur appelle `export_telemetry` ; le routage n'appelle jamais exporter
ni SDK vendor. Les adapters Prometheus/OpenTelemetry appartiennent au
déploiement et doivent borner leurs queues. Les compteurs stables 1.0 restent
inchangés ; le contrat détaillé est un ajout du RC.

**Maturité :** profil RC de peer transport pour la surface distribuee V1.
