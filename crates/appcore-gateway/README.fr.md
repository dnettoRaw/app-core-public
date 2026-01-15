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
secrets de production. HA du gateway, federation edge relay et transports
alternatifs restent futurs et ne doivent pas affaiblir l'authentification,
expiry, nonce ou replay protection de Peer RPC.

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

**Maturité :** profil RC de peer transport pour la surface distribuee V1.
