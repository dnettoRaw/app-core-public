# appcore-peer-rpc

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

À utiliser uniquement si tenant, cluster, source, cible, protocole, expiry,
nonce et intégrité sont établis. `AllowPeerAuthenticator` est réservé aux tests.

Le `Debug` des DTO peer request, response, outbound et HTTP expose les tailles
et omet bytes opaques, credentials, valeurs nonce/idempotence et details
d'erreur distante.

Avec protocole V2 explicitement sélectionné, `PeerRpcChunkEncoder` lit un chunk
borné depuis une source `Read` et émet les frames open/chunk/commit;
`PeerRpcChunkAssembler` vérifie et écrit un chunk décodé vers un sink `Write`.
La limite agrégée par défaut est 64 MiB. Toute entrée manquante, dupliquée,
réordonnée, corrompue, décompressée au-delà du quota, expirée ou annulée ferme
l'assembler définitivement. Un finish échoué abandonne le sink sans exposer
les bytes partiels comme validés.

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
`PeerRpcHttpHost::with_v2_stream_registry`. Le host par défaut reste V1-only.
`query_stream_v2` et `command_stream_v2` authentifient chaque body JSON exact et
déplacent request/response une frame à la fois. L'admission open valide tenant,
cluster, cible, trace, deadline, idempotence command et nonce replay. Les frames
ne sont jamais répétées après une panne transport ambiguë; l'annulation est
best effort et le nettoyage par deadline fait autorité.

La disponibilité du codec V2 n'est pas une négociation. L'appelant doit choisir
explicitement module et transport V2. `/v1/peer/*` analyse uniquement V1 et ne
fait aucun fallback automatique.

**Maturité :** V1 stable; transport V2 post-1.0 certifié en développement,
pas encore publié.

[Preuve de certification du stream V2 borné](benchmarks/peer-rpc-v2-2026-08-26.fr.md)
