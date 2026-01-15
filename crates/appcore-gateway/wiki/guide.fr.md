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

**Maturité :** profil RC de peer transport pour la surface distribuee V1.
