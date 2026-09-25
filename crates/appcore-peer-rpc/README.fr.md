# appcore-peer-rpc

L'entrée des corps V1, JSON V2 et binaires V2 partage 16 slots par hôte,
y compris entre routeurs obtenus séparément de cet hôte. La saturation renvoie
HTTP 503 avant la collecte ; la réception expire après 10 secondes (408).
V1 conserve son plafond HTTP de 2 MiB et V2 celui des frames du registre.
Health et manifest contournent cette admission. Ces échecs de transport avant
le décodage ne sont pas des réponses V2 signées. Les corps bruts admis sont
bornés à 16 fois le plus grand plafond activé, pas au RSS total. Décodage,
décompression, dispatch et réponses ont des limites distinctes.

Tests locaux:

```bash
cargo test -p appcore-peer-rpc
```

**Responsabilité :** client peer authentifié, host HTTP, validation et replay
protection.

**Dépendances internes :** core, distributed contracts, security et transport.

**API principale :** traits token issuer/authenticator/dispatcher et
implémentations HashToken/static; nonce stores mémoire/fichier; config,
validator et hashes ; retry/client config et trait transport ; transports
pooled et standard one-shot ; HTTP state et host.

Utilisez `PooledPeerRpcTransport` pour réutiliser des connexions bornées par
origine. `StdPeerRpcTransport` conserve le transport V1 one-shot.
Les deux transports prennent l'ownership de l'allocation du body du DTO HTTP.
Les bodies V1 non compressés et V2 exacts sont déplacés dans `HttpRequest` sans
clone intégral ; V1 crée un buffer gzip séparé uniquement s'il est plus petit.
Le client V1 déplace chaque payload outbound owned dans une enveloppe unique et
réutilise cet owner pendant les retries bornés. Chaque retry renouvelle toujours
timestamp, expiry, nonce, liaison de signature et body HTTP encodé. Avec un
payload raw de 4 Mio, cinq processus release sur Apple M1 ont maintenu le p50 à
+0,08 %, réduit le RSS de pic de 7,21 %, le delta RSS du workload de 9,02 % et
le delta retenu de 7,99 %.
À l'entrée, `decode_peer_rpc_envelope_json` contrôle la limite encodée et
désérialise un body V1 non compressé directement depuis les octets HTTP
empruntés. La composition root déplace ensuite l'allocation du payload décodé
vers `CommandEnvelope` ; aucune frontière ne retient un second payload complet.

Les nonce stores mémoire et fichier rejettent eux-mêmes les identifiants de
plus de 128 octets. Le store fichier décode son état V1 owner-only avec un
reader limité à 16 Mio et sérialise directement la map conservée avec un buffer
fixe de 64 Kio dans un temporaire exclusif. Corruption complète, champs
inconnus, clés invalides et état oversized échouent fermés ; une écriture
échouée supprime son stage. Sous Windows, le remplacement utilise l'opération
atomique write-through de la plateforme.

Le dispatch V1/V2 partage 16 permits blocking dans le processus. Le host
utilise au plus 16 threads avec des stacks de 1 Mio et retire les threads
inactives après cinq secondes. La saturation précède la file Tokio ; V2 renvoie
`CapacityExceeded`.

Le contrat opt-in `v2`, `PeerRpcChunkEncoder` et `PeerRpcChunkAssembler`
traitent sources et sinks importants un chunk borné à la fois. Les limites par
défaut sont 64 KiB décodés par chunk, 96 KiB encodés, 64 MiB au total et 1 024
chunks. Séquence, tailles exactes, hash par chunk et total, deadline, annulation
et quota après décompression échouent de manière fermée. Ces API codec ne
clonent pas les chunks incompressibles : l'allocation owned passe de la source à
la frame puis au receiver. Une sonde fixe sur stack sur le chunk évite le
gzip spéculatif pour les données probablement déjà compressées ; les chunks
compressibles utilisent toujours gzip. Elles ne sélectionnent pas
automatiquement le transport V2; les routes V1 n'infèrent jamais V2.

`PeerRpcStreamRegistry` ajoute des quotas exacts de sessions et d'octets
décodés, des spools exclusifs réservés au propriétaire, des pulls bornés pour
la réponse du dispatcher et des compteurs de saturation/nettoyage. Erreur,
annulation, expiration et fin libèrent fichier partiel et réservation.
Unix exige le propriétaire effectif et les modes répertoire/fichier
`0700`/`0600`. Windows rejette les reparse points et tout allow ACE hors du SID
propriétaire du processus courant. Les autres plateformes refusent le spool.

HTTP V2 n'est installé que par `PeerRpcHttpHost::with_v2_stream_registry`.
JSON reste le codec par défaut. Le host appelle aussi
`with_v2_binary_codec` et le client utilise `with_stream_codec_v2(Binary)` pour
les routes Postcard séparées et les octets de chunk natifs. Chaque body exact
sélectionné est lié à un nouveau bearer token et traité incrémentalement. Le
JSON canonique est sérialisé directement dans SHA-256 pour cette liaison, sans
conserver un second body encodé complet à côté de la frame. Le helper public
`json_payload_hash` fournit le même chemin byte-exact aux dépendants. Les
bodies binaires sont limités à 256 Kio et jamais compressés par HTTP; le gzip
borné par chunk reste dans la frame signée. Un support binaire absent ou
incompatible est terminal et ne déclenche jamais de fallback JSON. L'open
réutilise les validations tenant, cluster, cible, trace, deadline et nonce
replay; les commands exigent l'idempotence. Les frames ne sont pas répétées
après une panne transport ambiguë. V1 reste la surface par défaut sans upgrade
automatique.

[Preuve clean-source de certification V2 64 MiB](wiki/benchmarks/peer-rpc-v2-2026-08-26.fr.md)

À utiliser uniquement si tenant, cluster, source, cible, protocole, expiry,
nonce et intégrité sont établis. `AllowPeerAuthenticator` est réservé aux tests.

Le `Debug` des DTO peer request, response, outbound et HTTP expose les tailles
et omet bytes opaques, credentials, valeurs nonce/idempotence et details
d'erreur distante.

`BoundedReplayStore` valide les nonces à la limite partagée de 128 octets et
borne les entrées actives comme les octets retenus estimés. Son plafond dérivé
ne dépasse jamais 32 Mio ; `with_max_bytes` choisit une limite plus stricte et
`memory_metrics` expose octets courants, pic, maximum et rejets sans révéler les
nonces.

La matrice V2 expose aussi les résultats stables `capability_not_found`,
`peer_busy`, `timeout`, `stale`, `incompatible` et `transport_unavailable` pour
la classification par l'UI et les opérateurs. Les messages appartiennent au
protocole et sont expurgés.

`PeerRpcStreamRegistry::set_capability_limits` associe à une capability des
limites explicites d'octets décodés de requête, d'octets de réponse et de délai.
Les capabilities non enregistrées conservent uniquement les limites générales
du registry; aucun élargissement implicite de policy n'existe. Une requête ou
réponse rejetée est supprimée avant publication.

`PeerRpcChunkTransferRequestV2` et `PeerRpcChunkTransferResponseV2`, indépendants
de update, fournissent un transfert borné par hash d'objet, offset et longueur.
`serve_chunk` ne lit que la plage demandée depuis une source seekable, et
`verify_chunk` vérifie identité, métadonnées de plage et digest avant le commit
par un sink reprenable.

**Maturité :** V1 stable; transport V2 post-1.0 certifié en développement.

## Documentation stable

Identifiant stable : **ACR-017**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-017). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
