# appcore-peer-rpc

L'entrée des corps V1, JSON V2 et binaires V2 partage 16 slots par hôte,
y compris entre routeurs obtenus séparément de cet hôte. La saturation renvoie
HTTP 503 avant la collecte ; la réception expire après 10 secondes (408).
V1 conserve son plafond HTTP de 2 MiB et V2 celui des frames du registre.
Health et manifest contournent cette admission. Ces échecs de transport avant
le décodage ne sont pas des réponses V2 signées. Les corps bruts admis sont
bornés à 16 fois le plus grand plafond activé, pas au RSS total. Décodage,
décompression, dispatch et réponses ont des limites distinctes.

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** client peer authentifié, host HTTP, validation et replay
protection.

**Dépendances internes :** core, distributed contracts, security et transport.

**API principale :** traits token issuer/authenticator/dispatcher et
implémentations HashToken/static; nonce stores mémoire/fichier; config,
validator et hashes ; retry/client config et trait transport ; transports
pooled et standard one-shot ; HTTP state et host.

Utilisez `PooledPeerRpcTransport` pour réutiliser des connexions bornées par
origine. `StdPeerRpcTransport` conserve le comportement V1 one-shot avec
`Connection: close`.
Les deux consomment l'allocation owned du body du DTO HTTP : un body V1 non
compressé ou une frame V2 exacte conserve la même allocation `Vec<u8>` à
travers `HttpRequest`, sans garder un body cloné à côté.

Le client V1 déplace un payload outbound owned dans une enveloppe et conserve
cet owner pendant les retries bornés. Chaque retry génère toujours de nouveaux
champs temporels et nonce, signe l'enveloppe renouvelée et encode un nouveau
body HTTP. Un workload de 4 Mio sur cinq processus release Apple M1 a maintenu
le p50 à +0,08 %, réduit le RSS de pic de 7,21 %, le delta RSS du workload de
9,02 % et le delta RSS retenu de 7,99 %.

À l'entrée, `decode_peer_rpc_envelope_json` applique le plafond d'octets encodés
avant le parse et emprunte directement le body HTTP non compressé. La
composition root du Runtime déplace aussi le payload V1 décodé dans
`CommandEnvelope`. Avec un payload raw de 4 Mio sur Apple M1, cinq processus
release ont réduit le p50 de 53,06 à 52,35 ms, le RSS de pic de 35,80 à
23,81 Mio et le delta RSS de workload de 39,52 %.

Le dispatch V1/V2 partage 16 permits blocking et un pool Tokio au même plafond,
avec des stacks de 1 Mio et retrait après cinq secondes d'inactivité. Un gate
plein rejette avant la file ; V2 utilise `CapacityExceeded`.

À utiliser uniquement si tenant, cluster, source, cible, protocole, expiry,
nonce et intégrité sont établis. `AllowPeerAuthenticator` est réservé aux tests.

Le `Debug` des DTO peer request, response, outbound et HTTP expose les tailles
et omet bytes opaques, credentials, valeurs nonce/idempotence et details
d'erreur distante.

Utilisez `FilePeerNonceStore` uniquement dans son répertoire privé au owner. Il
accepte au plus 65 536 entrées actives, limite chaque nonce à 128 octets et le
fichier V1 à 16 Mio. Le chargement décode directement depuis le reader borné ;
chaque request acceptée réécrit la map ordonnée avec un buffer fixe de 64 Kio
et un remplacement atomique, sans `Vec` JSON encodé. Le startup rejette champs
inconnus, clés invalides, corruption complète et fichiers oversized. Le
benchmark du crate valide le nombre maximal d'entrées avec les phases RSS
idle/workload/retained.

`BoundedReplayStore` applique la même validation des nonces à la protection de
replay locale au processus. Le nombre et les octets retenus estimés sont tous
deux bornés ; le plafond par défaut dérive de la politique d'entrées et ne
dépasse jamais 32 Mio. `with_max_bytes` permet un plafond plus strict.
`memory_metrics` expose octets courants, pic, maximum et rejets sous pression,
sans exposer les nonces.

Avec protocole V2 explicitement sélectionné, `PeerRpcChunkEncoder` lit un chunk
borné depuis une source `Read` et émet les frames open/chunk/commit;
`PeerRpcChunkAssembler` vérifie et écrit un chunk décodé vers un sink `Write`.
La limite agrégée par défaut est 64 MiB. Toute entrée manquante, dupliquée,
réordonnée, corrompue, décompressée au-delà du quota, expirée ou annulée ferme
l'assembler définitivement. Un finish échoué abandonne le sink sans exposer
les bytes partiels comme validés. Pour un chunk identity, encoder et assembler
déplacent la même allocation owned au lieu de cloner les octets décodés à
chaque frontière. Une sonde fixe sur stack sur tout le chunk évite le gzip
spéculatif uniquement s'il semble déjà incompressible ; les chunks structurés
compressibles utilisent toujours gzip.

`PeerRpcStreamRegistry` possède les sessions V2 partielles sous des quotas
explicites de sessions et d'octets décodés. Les requêtes utilisent des fichiers
exclusifs dans un répertoire de spool existant réservé au propriétaire; seuls
les payloads entièrement vérifiés atteignent le dispatcher et les réponses
utilisent des pulls explicites et bornés. Erreur, annulation, expiration et fin
suppriment fichier et réservation. Le snapshot expose sessions, octets réservés,
saturations et nettoyages.
Unix valide le propriétaire effectif et les modes répertoire/fichier
`0700`/`0600`. Windows rejette les reparse points et tout allow ACE hors du SID
propriétaire du processus courant. Les autres plateformes échouent fermées.

Installez HTTP V2 explicitement avec
`PeerRpcHttpHost::with_v2_stream_registry`. Le host par défaut reste V1-only et
V2 utilise JSON canonique par défaut. Le framing binaire exige l'opt-in séparé
`with_v2_binary_codec` du host et la sélection
`with_stream_codec_v2(PeerRpcStreamCodecV2::Binary)` du client. Il utilise des
paths query/command distincts et le media type exact
`application/vnd.appcore.peer-rpc.v2+postcard`. Chaque body sélectionné exact
est authentifié et request/response avance une frame à la fois.
Le JSON canonique est sérialisé directement dans SHA-256 pour la liaison du
token, sans conserver un second body encodé complet à côté de la frame.
Les dépendants réutilisent ce chemin byte-exact via `json_payload_hash`.
Les bodies binaires ne reçoivent jamais gzip HTTP et restent sous 256 Kio;
gzip optionnel
du chunk est toujours décodé sous la limite déclarée. Route absente, mismatch
de media type ou reply malformée est terminal, sans fallback JSON. L'admission open valide tenant,
cluster, cible, trace, deadline, idempotence command et nonce replay. Les frames
ne sont jamais répétées après une panne transport ambiguë; l'annulation est
best effort et le nettoyage par deadline fait autorité.

Les corps de rejet V2 utilisent `PeerRpcWireErrorV2`. Le client valide code,
phase, retryability, délai, corrélation et message contrôlé par le protocole
comme une matrice unique avant de retourner
`PeerRpcStreamClientErrorV2::Remote`. Les codes inconnus sont observables mais
terminaux et expurgés. Les rejets V1 deviennent
`PeerRpcError::RemoteRejected` par égalité exacte; disponibilité et capacité
de replay sont les seuls cas V1 distants avec retry. Aucun chemin n'interprète
une sous-chaîne.

La disponibilité du codec V2 n'est pas une négociation. L'appelant doit choisir
explicitement module et transport V2. `/v1/peer/*` analyse uniquement V1 et ne
fait aucun fallback automatique.

**Maturité :** V1 stable; transport V2 post-1.0 certifié en développement.

[Preuve de certification du stream V2 borné](benchmarks/peer-rpc-v2-2026-08-26.fr.md)
