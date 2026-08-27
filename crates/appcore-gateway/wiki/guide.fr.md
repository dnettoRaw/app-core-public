# appcore-gateway

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

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
> compilation ; utilisez `tenant_partition`,
> `tenant_partition_or_insert`, `tenant_count` et `connection_count`. Les maps
> de requests en attente sont désormais privées ; utilisez
> `pending_request_count` pour l'observation et laissez `EnvelopeRouter` gérer
> leur cycle de vie. Aucun alias historique ni map miroir n'est fourni. La
> migration complète se trouve dans `release/gateway-tenant-migration.md`.

Le gateway résout le tenant depuis le suffixe de domaine défini par le
deployment ou depuis un paramètre de query réservé aux tests locaux, authentifie
les connexions lorsque configuré, route les enveloppes Peer RPC et les requests
HTTP Peer RPC via mesh relay uniquement dans la partition du tenant et retire
les workers stale avec des files de sortie bornées.

Le chemin normal d'activation du Runtime utilise la map d'adapters du
Deployment Manifest :

```toml
[adapters.gateway]
provider_id = "appcore-gateway"
settings = { bind_address = "127.0.0.1:8080", domain_suffix = "gateway.example.com", heartbeat_interval_ms = "30000", heartbeat_timeout_ms = "90000" }
secret_refs = {}
```

Le mode cluster exige aussi `paths.gateway_replay` absolu, un fichier sur un volume
partage et inscriptible par toutes les instances Gateway.

Le parser accepte uniquement ces quatre settings non secretes. Les endpoints,
references de secret, settings inconnues et overrides d'authentification
echouent fermes. `appcore-bin` ajoute et autorise le descriptor owner
`runtime.gateway` dans le catalogue partage, reutilise la securite du Runtime
et enregistre l'instance comme service critique du Supervisor. Sans
`adapters.gateway`, aucun runtime, listener ou task Gateway n'existe.

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
secrets de production. HA du gateway, federation edge relay et transports
alternatifs restent futurs et ne doivent pas affaiblir l'authentification,
expiry, nonce ou replay protection de Peer RPC.

Le host utilise un `FilePeerNonceStore` durable et sur entre processus :
standalone le garde dans le storage prive, tandis que cluster echoue ferme sans
`paths.gateway_replay` absolu sur un fichier partage et inscriptible. Les sockets
expirent apres 60 secondes maximum. Les embedders peuvent injecter un autre
`PeerNonceStore`; leur defaut reste local et borne. Le rate limit par IP source
et la terminaison TLS restent au deployment.

`GatewayRuntime` possede le listener, le runtime Tokio current-thread, le
router, le pruner heartbeat et la thread. Le startup bind synchronement : une
adresse invalide ou occupee arrete donc le host. Le shutdown cooperatif borne
joint tout le travail. Avant le delai, il abandonne le future serveur et ferme
les connexions lentes ou incompletes avant de joindre la thread. `Orphaned`
reste seulement une quarantaine defensive de panne thread. Les snapshots surs
contiennent uniquement lifecycle, adresses de bind et compteurs. Les utilisateurs directs de `spawn_heartbeat_pruner` doivent
conserver et attendre le join handle retourne.

Les hashes de connexion worker et client utilisent un framing binaire
canonique V2 avec le marqueur `v2:`. Les anciens hashes sans version ne sont
pas interchangeables; émetteurs de token et consommateurs Gateway doivent être
mis à jour ensemble.

Chaque tenant conserve des index directs et bornés par Core ID et par
`(cluster_id, core_id)`. Le lookup de routage est O(1). Register, reconnect,
disconnect et prune heartbeat mettent à jour map primaire, registre de
capabilities et index sous le même verrou tenant. Des compteurs saturés de
rebuild et d'incohérence exposent la santé sans labels non bornés.

## Ownership du registre HA (contrat `1.0.2-rc`)

`GatewayRegistryProvider` définit des leases asynchrones par tenant pour
l'instance, l'ownership worker/session, une résolution bornée et le
claim/completion des requests en vol. `GatewayInstanceLease` porte un epoch
monotone; `GatewayWorkerRecord` lie aussi la génération de connexion locale;
et `GatewayRequestFence` lie epochs origin/target et génération worker. Chaque
mutation doit comparer atomiquement ces valeurs.

`GatewayFederationUrl` accepte HTTPS ou HTTP loopback uniquement, rejette les
credentials intégrés et expurge sa valeur du `Debug`. Les records request et
session omettent aussi leurs identités du debug.

`RedisGatewayRegistryProvider` implémente maintenant ce contrat. Configurez-le
avec `RedisGatewayRegistryConfig`, convertissez le `ResolvedSecret` du
déploiement avec `RedisGatewayCredential::new(secret.into_zeroizing())`, puis
fournissez cet owner à `connect`; aucun credential n'est accepté dans l'endpoint.
Redis sans TLS est limité au loopback et les endpoints distants exigent
`rediss://`. Timeout maximal 5 secondes, concurrency maximale 64, leases
instance/worker au plus 60 secondes et résolution au plus 1 024 workers. Les
scripts par tenant imposent 1 024 workers, 4 096 sessions et 2 048 requests en
attente.

Une incertitude transport retourne `Unavailable` sans rejouer une mutation
ambiguë. Le propriétaire du lifecycle doit entrer en isolation et appeler
explicitement `reconnect` avant de reprendre un epoch supérieur.
`GatewayHaLifecycle` expose les modes fixes `Stopped`, `Recovering`, `Healthy`
et `Isolated`, ainsi que des compteurs bornés transition/recovery/fencing.
L'attacher via `GatewayState::with_ha_lifecycle` ferme admission HTTP/WebSocket,
dispatch request et completion response hors de `Healthy`. Un état sans ce
lifecycle conserve le comportement single-instance.

`GatewayHaCoordinator` possede une liste fixe, unique et bornee de bindings
tenant/cluster pour une instance. Il acquiert tous les epochs avant `Healthy`,
renouvelle l'ensemble exact, annule les acquisitions terminees apres une panne
partielle et efface les leases locaux lors d'un renewal stale ou incertain. Les
rounds sont serialises, utilisent au plus 64 operations provider en parallele
et ont une deadline totale de cinq secondes. Sa boucle cooperative retente le
recovery en isolation et libere les leases exacts apres fermeture de
l'admission.

`GatewayRuntime::with_ha_coordinator` possede cette boucle et fournit le
snapshot local. Le recovery reenregistre chaque worker live borne et session
non expiree avant `Healthy`. Un nouveau socket entre dans le registre partage
avant admission locale; disconnect, prune heartbeat et shutdown suppriment le
record exact. La telemetrie snapshot expose seulement lifecycle et compteurs
fixes d'ownership.

Le chemin local claim maintenant les epochs origin/target et la generation
worker avant dispatch, complete le fence avant de retourner un succes et
annule apres panne de queue, timeout ou shutdown. Un future de route abandonne
par son owner ne laisse qu'un record provider borne par le TTL request de 30
secondes. La target peut verifier le claim live exact sans le consommer avant
l'admission. Des compteurs fixes exposent claims, completions et cancellations
sans labels request.
Le schema strict federation V2 lie ce fence et la request interne a un
credential separe a usage unique et retourne des erreurs AC-021 typees. La
route HTTP bornee passe un E2E avec deux etats Gateway et complete le fence
avant d'accepter la reponse. La preuve deployment combinee utilise Redis 7.4 et
Caddy 2.11.4 sans bypass direct de l'origin, perd brutalement l'owner et route a
nouveau via Caddy avec un epoch superieur apres le TTL borne du lease. La
certification plateforme reste en attente.

Le harness local AC-022 mesure aussi le lookup partage et le recovery complet
avec 1, 100 et 1 000 tenants, puis 64 routes reussies pour chacun des chemins
local et federe. Il utilise un provider en processus pour isoler l'overhead du
contrat; la preuve combinee Redis, proxy et perte d'owner reste un test
deployment ignore separe.
Une preuve CI de plateforme reste requise avant de qualifier le profil a deux
instances de deployable. Le repertoire local ne devient jamais une verite de
fallback.

## Sélection des workers (`1.0.3-rc`)

`FirstAvailable` reste le défaut compatible et utilise désormais un ordre
d'identité stable. Les policies opt-in `RoundRobin`, `LeastInflight`,
`HealthWeighted` et `Affinity` opèrent uniquement sur le registre de capability
du tenant courant. Utilisez le selector live avant de construire et signer la
cible Peer RPC explicite :

```rust
use appcore_gateway::{
    CapabilityResolver, WorkerSelectionInput, WorkerSelectionPolicy,
};
use std::time::Duration;

tenant.resolver = CapabilityResolver::with_policy(WorkerSelectionPolicy::LeastInflight);
let selected = tenant.select_worker(
    &capability,
    WorkerSelectionInput::new(now_ms, Duration::from_secs(90)),
)?;
```

L'enum V1 exhaustif `SelectionPolicy` reste limité à `FirstAvailable`.
Les policies avancées utilisent le nouveau `WorkerSelectionPolicy` non
exhaustif ; la compatibilité source des consommateurs V1 stables est préservée.

Toutes les policies live rejettent les workers fermés/stale, les files de
sortie pleines et les workers à leur limite inflight. Health weighting utilise
des poids fixes de 1 à 16 selon l'âge du heartbeat. Affinity accepte au plus
128 octets et utilise un rendezvous hashing stateless par tenant, sans
conserver de map de clés. Le dispatch réel acquiert indépendamment un permit de
64 routes par worker et le libère en cas de succès, échec, timeout, annulation
et shutdown. Le Gateway ne réécrit jamais la cible V1 signée et n'effectue
aucun fallback silencieux de policy.
Les mesures de référence propres sont consignées dans le
[benchmark de sélection des workers Gateway](benchmarks/gateway-worker-selection-2026-08-26.fr.md).

## Télémétrie bornée par capability (`1.0.4-rc`)

Chaque route met à jour un outcome fixe et des histogrammes fixes de latence
complète, attente worker, attente du verrou tenant et octets du payload opaque.
Le snapshot processus indique aussi inflight/pic, pic de queue, reconnects,
retries explicites, échecs d'authentification, rejets unhealthy/capacity, pic
inflight worker, overflow et échecs exporter. Les percentiles sont les bornes
supérieures des buckets, pas des échantillons conservés.

Le registre conserve 128 labels de capability validés et une série d'overflow
fixe. Il ne crée jamais de label tenant, installation, Core, request,
connexion, token, payload ou erreur dynamique. `GatewayTelemetryExporter` est
une frontière pull : l'appelant lui passe explicitement un snapshot immuable
hors des verrous de routage. Un échec exporter incrémente `export_failures` et
revient uniquement à cet appelant ; il ne rejette ni ne ralentit une route car
le routage ne l'appelle jamais.

Le gate release exécute 4 096 routes rejetées instrumentées et 256 snapshots à
cardinalité maximale. Les budgets sont 1 ms p99 par route et 5 ms p99 par
snapshot. Les adapters Prometheus/OpenTelemetry consomment le même contrat hors
du crate et possèdent leurs queues, retry et policy transport.
Les mesures de référence propres sont consignées dans le
[benchmark de télémétrie Gateway](benchmarks/gateway-telemetry-2026-08-26.fr.md).

**Maturité :** profil RC de peer transport V1 ; la télémétrie détaillée est un
contrat RC actuel.
