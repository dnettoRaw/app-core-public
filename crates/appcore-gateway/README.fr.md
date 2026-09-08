# appcore-gateway

[English guide](wiki/guide.en.md) |
[Guia em português](wiki/guide.pt.md) |
[Guide français](wiki/guide.fr.md)

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

Le répertoire privé conserve 32 générations de shards immuables en
copy-on-write. Les scans globaux d'admission, heartbeat et HA ne copient que 32
handles `Arc`, libèrent tous les verrous de shard, puis inspectent les
partitions partagées. Ils n'allouent ni ne clonent une liste complète de tenants.

## Architecture

```text
Browser / Client
      |
HTTPS / WebSocket (JSON PeerRpcEnvelope / PeerRpcResponse)
      |
*.<deployment-domain>
      |
AppCore Gateway
      |
WebSocket / RPC / mesh-relay
      |
Workers
```

Le déploiement définit `domain_suffix` dans `GatewayConfig::new(bind_address,
"gateway.example.com")`. Une requête vers `tenant-a.gateway.example.com`
résout le tenant `tenant-a`. TLS appartient à l'infrastructure du déploiement.

## Composition dans le Runtime

La composition Gateway appartient au déploiement. Un déploiement active cette crate avec
la map d'adapters existante :

```toml
[adapters.gateway]
provider_id = "appcore-gateway"
settings = { bind_address = "127.0.0.1:8080", domain_suffix = "gateway.example.com", heartbeat_interval_ms = "30000", heartbeat_timeout_ms = "90000" }
secret_refs = {}
```

Le manifeste déclare la configuration ; il ne démarre pas le Gateway.
L'intégration de déploiement transmet le provider sélectionné à
`GatewayConfig::from_provider_config`, qui accepte uniquement les quatre
settings ci-dessus et rejette endpoints, références de secret, settings
inconnues et tentatives de désactiver l'authentification.

Le déploiement possède l'autorisation des capabilities, le provider de sécurité,
l'injection explicite du replay store, l'enregistrement au Supervisor,
le démarrage et l'arrêt. `GatewayRuntime::new` crée un runtime arrêté avec
une protection antireplay bornée et locale au processus. Pour le replay après
redémarrage ou entre instances, utilisez explicitement
`GatewayRuntime::with_replay_store` ; la composition HA utilise
`GatewayRuntime::with_ha_coordinator` avec store et coordinator.
Un chemin de manifeste ne construit pas ces objets. Un fichier partagé
convient uniquement si son filesystem respecte le contrat de sûreté entre
processus du store ; un fichier local ne coordonne pas des hosts distincts.

Le SDK n'effectue pas cette composition. Le déploiement doit propager une
configuration invalide et les échecs de démarrage/bind ; déclarer la
configuration seule ne crée ni listener ni task.

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

Le RC actuel inclut le contrat `GatewayRegistryProvider`. Le build par defaut
conserve ce contrat et tout le Gateway a instance unique sans lier un client
Redis. Activez la feature Cargo additive `ha-redis` uniquement dans
l'integration qui compose le provider HA Redis :

```toml
appcore-gateway = { version = "2.0.0-alpha.2", features = ["ha-redis"] }
```

`RedisGatewayRegistryProvider` implemente le contrat. Elle exige TLS hors loopback, un credential
resolu separement, des limites timeout/concurrency et des scripts atomiques
dans le hash slot du tenant. Une mutation au resultat ambigu n'est jamais
rejouee; le caller entre en isolation et appelle explicitement `reconnect`.
`GatewayHaLifecycle` ferme deja admission HTTP/WebSocket, dispatch et
completion hors de `Healthy`, sans modifier le mode single-instance.
`GatewayHaCoordinator` acquiert et renouvelle maintenant l'ensemble complet et
borne des leases tenant avant `Healthy`; un round partiel, stale ou incertain
efface les leases locaux et entre en `Isolated`. Chaque round est serialise,
limite a 64 operations concurrentes et cinq secondes au total. Le renewal
partage des snapshots immuables des leases et workers au lieu de copier toutes
les identites avant chaque round ; les mutations locales utilisent
copy-on-write dans la frontiere serialisee du coordinator.
`GatewayRuntime::with_ha_coordinator` possede la task recovery/shutdown, rejoue
le snapshot complet et borne workers/sessions avant `Healthy`, enregistre les
nouveaux sockets avant admission locale et supprime les records exacts au
disconnect ou prune heartbeat. Le chemin local claim maintenant les epochs
origin/target et la generation worker avant dispatch, complete avant de
retourner un succes et annule sur panne de queue, timeout ou shutdown; un future
abandonne expire sous 30 secondes. Le provider peut verifier le claim live exact
sans le consommer avant l'admission target. La route V2 de federation est implémentée ;
le schema strict lie maintenant body, epochs, generation et credential separe
a usage unique, avec erreurs AC-021 typees. La route HTTP bornee passe un E2E
avec deux etats Gateway et complete le fence avant d'accepter la reponse; la
meme preuve passe avec Redis 7.4 et via Caddy 2.11.4 sans bypass direct de
l'origin. Le recovery apres perte de l'owner route aussi avec un epoch superieur
apres le TTL borne. AC-022 et les preuves plateforme restent obligatoires avant
le deploiement HA; aucun fallback local n'est permis.

Les embedders utilisent par défaut un replay store borné et local au processus.
Pour protéger les tokens de connexion contre le replay après redémarrage ou
entre instances, le deployment doit injecter un `PeerNonceStore` approprié via
`GatewayState::with_replay_store` ou `GatewayRuntime::with_replay_store`.
`FilePeerNonceStore` est sûr entre processus selon son contrat de filesystem ;
un fichier local seul ne coordonne pas plusieurs hosts. Le host Runtime supprimé
ne configure plus de store ni `paths.gateway_replay`. Les sockets actifs expirent
avec leurs credentials sous 60 secondes. Les leases HA et le request fencing ne
remplacent pas l'admission antireplay des tokens de connexion. La limitation par
IP source et la terminaison TLS restent au deployment.

`GatewayRuntime` possede listener, thread de runtime, router et pruner. `stop`
demande d'abord un shutdown graceful, puis abandonne le future serveur avant le
delai pour fermer les connexions incompletes et joindre la thread. `Orphaned`
reste une quarantaine defensive pour une panne inattendue de thread, pas le
chemin normal d'un timeout. Le snapshot n'expose jamais credentials ou tokens.
Les embedders qui appellent directement `spawn_heartbeat_pruner` possèdent son
join handle et doivent attendre sa terminaison.

Le runtime et le transport de fédération utilisent au plus 16 threads blocking
et 16 permits avant admission, avec des stacks de 1 Mio et retrait après cinq
secondes d'inactivité. La saturation annule le request fence avant la file.
Chaque request de fédération admise est déplacée dans ce worker blocking, puis
son buffer JSON encodé est déplacé dans la request HTTP. Le corps Peer RPC
interne ne conserve donc pas deux copies intégrales supplémentaires du payload.
Le hash du credential externe utilise `json_payload_hash` pour sérialiser le
JSON canonique directement dans SHA-256 avant de créer l'unique buffer wire
requis ; aucun autre `Vec` réservé au hash n'est conservé.

Les hashes de connexion worker et client utilisent un framing binaire
canonique V2 avec le marqueur `v2:`. Les anciens hashes sans version ne sont
pas interchangeables; émetteurs de token et consommateurs Gateway doivent être
mis à jour ensemble.
Le hash emprunte ses champs de validation. Le framing maximal des capabilities
worker écrit directement dans sa sortie hexadécimale finale de 17 Kio, sans
l'ancien frame binaire de 8,5 Kio ni une seconde chaîne de 17 Kio réservée au
hash. Le parser conserve un nom validé owned et une entrée de déduplication
empruntée par capability unique.

Chaque tenant conserve des index workers directs et bornés par Core ID et par
`(cluster_id, core_id)`. Le lookup courant avec un Core unique est O(1) ; les
Core IDs dupliqués utilisent un scan borné par le plafond de workers du tenant.
Register, reconnect, disconnect et prune heartbeat mettent à jour map, registre
de capabilities et index sous le même verrou tenant. `worker_index_rebuilds` et
`worker_index_inconsistencies` exposent des compteurs bornés de santé d'index.

Le registre de capabilities conserve un seul owner partagé du nom pour chaque
capability distincte du tenant, au lieu d'une allocation de chaîne par annonce
worker. La map directe capability→workers reste la source de vérité du routage.
`capabilities_for_iter` lit sans clone les noms ordonnés d'un worker et `stats`
rapporte uniquement noms distincts, workers, annonces et octets UTF-8 uniques.
La deregistration libère le nom au départ du dernier annonceur ; les clones du
registre partagent les noms immuables mais gardent des index indépendants.

## Sélection déterministe des workers dans `1.0.3-rc`

L'enum V1 exhaustif `SelectionPolicy` reste limité à `FirstAvailable`.
`WorkerSelectionPolicy` fournit les choix opt-in `RoundRobin`, `LeastInflight`,
`HealthWeighted` et `Affinity`, tandis que `FirstAvailable` reste le défaut.
Les consommateurs RC des variantes avancées doivent modifier le nom de l'enum ;
aucun manifeste ni contrat wire ne change. L'ordre d'identité des candidats est stable et ne dépend pas de l'itération
d'un `HashSet`. `CapabilityResolver::select` reçoit des entrées live bornées et
rejette capability absente, worker stale/déconnecté, worker épuisé et affinity
invalide avec des valeurs `WorkerSelectionError` distinctes.
La sélection emprunte les identités : first-available, least-inflight et
affinity ne conservent aucune liste de candidats ; round-robin et
health-weighted utilisent un seul buffer compact emprunté pour préserver
l'ordre stable. Seule la clé retenue est clonée pour le résultat public owned.
Le lookup des candidats ne clone plus les IDs d'installation et de Core pour
construire une clé tuple temporaire. Il utilise l'index Core existant dans le
cas courant et un scan exact borné lorsque plusieurs installations partagent un
Core ID. Sur des exécutions équivalentes de la certification Gateway complète,
les allocations ont baissé de 89,10 %, les octets demandés de 45,21 % et le p99
de sélection de 15,63 % à 20,87 %.

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

## Documentation stable

Identifiant stable : **ACR-018**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-018). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
