# appcore-peer-rpc

**Responsabilité :** client peer authentifié, host HTTP, validation et replay
protection.

**Dépendances internes :** core, distributed contracts, security et transport.

**API principale :** traits token issuer/authenticator/dispatcher et
implémentations HashToken/static; nonce stores mémoire/fichier; config,
validator et hashes ; retry/client config et trait transport ; transports
pooled et standard one-shot ; HTTP state et host.

Utilisez `PooledPeerRpcTransport` pour réutiliser des connexions bornées par
origine. `StdPeerRpcTransport` conserve le transport V1 one-shot.

Le contrat opt-in `v2`, `PeerRpcChunkEncoder` et `PeerRpcChunkAssembler`
traitent sources et sinks importants un chunk borné à la fois. Les limites par
défaut sont 64 KiB décodés par chunk, 96 KiB encodés, 64 MiB au total et 1 024
chunks. Séquence, tailles exactes, hash par chunk et total, deadline, annulation
et quota après décompression échouent de manière fermée. Ces API codec ne
sélectionnent pas automatiquement le transport V2; les routes V1 n'infèrent
jamais V2.

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
sélectionné est lié à un nouveau bearer token et traité incrémentalement. Les
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

**Maturité :** V1 stable; transport V2 post-1.0 certifié en développement,
pas encore publié.
