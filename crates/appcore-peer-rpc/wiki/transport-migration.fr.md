# Enveloppe applicative V1 vers Peer RPC streaming V2

Versions d'enveloppe/schema applicatif, wire sync et transport Peer RPC sont
indépendantes. Streaming V2 ne modifie ni schema métier ni réplication
leader-follower. Chunks/streams existent déjà ; migrer exige des adaptateurs
applicatifs externes explicites.

## Mapping et contrat de deployment

| Frontière applicative | Frontière streaming | Responsabilité conservée |
| --- | --- | --- |
| Négociation / status V1 | Query de capability applicative versionnée | Déclarer transport supporté ; absence signifie non éligible V2 |
| `IngestChunkV1` | Stream open/chunk/commit vérifié avec payload applicatif | Adaptateur externe reconstruit/valide octets, identité, séquence et digest |
| `PullRequestV1` / `PullChunkV1` | Command/query explicite et réponse pull bornée | Conserver curseur, checkpoint et autorisation hors frames |
| Pages snapshot / événements | Payloads opaques bornés | Application valide schema et publie seulement un résultat complet vérifié |

Le package de protocole externe doit contenir le mapping et les fixtures golden. Conserver
les endpoints applicatifs V1 pour anciens clients seulement dans ce package.
Runtime V1 n'infère pas V2, ne lit pas d'enveloppes supprimées et ne convertit
pas les formats applicatifs. Les entrées Runtime supprimées retournent toujours
`NO MORE SUPPORTED PLEASE UPDATE`.

## Migration par étapes

1. Inventorier versions installées, identités, scopes, limites, retries et
   outboxes/checkpoints persistés. Garder fixtures V1 dans le dépôt applicatif ;
   aucun schema métier dans Runtime.
2. Ajouter une capability versionnée explicite à l'adaptateur externe.
   Configurer `PeerRpcStreamRegistry` dans un spool existant owner-only et
   installer via `PeerRpcHttpHost::with_v2_stream_registry`. Définir budgets par
   capability ; valider tenant, cluster, origine et cible avant dispatch.
3. Les clients applicatifs V1 gardent leur chemin explicite actuel. Sélectionner
   V2 après négociation validée ou setting deployment explicite. L'hôte Runtime
   par défaut reste V1. JSON canonique V2 est distinct du binaire : ce dernier
   exige hôte `with_v2_binary_codec` et client
   `with_stream_codec_v2(PeerRpcStreamCodecV2::Binary)` ensemble.
4. Prouver payload/résultat et checkpoints équivalents par transport. Tester
   timeout, annulation, replay, duplication, corruption, oversize, isolation,
   perte de pair et commit ambigu. Ne pas répéter automatiquement les frames
   après panne ambiguë ; réconcilier receipts avec la même clé d'idempotence.
5. Déployer les receivers d'abord, puis senders sélectionnés. Observer codes
   sûrs, saturation, nettoyage du spool et retard checkpoint. Cohortes explicites ;
   aucun downgrade silencieux après erreur d'authentification ou media type.
6. Rollback : arrêter nouveaux sends V2, drainer/expirer sessions, réconcilier
   receipts et préserver outboxes/checkpoints. Reprendre l'adaptateur externe V1
   explicitement avec schema/état compatibles. Persistance incompatible exige
   plan version/migration/rollback revu ; ni effacement ni conversion inférée.

## Preuves de conclusion

Preuves applicatives : ancien client/nouveau receiver, nouveau sender/nouveau
receiver, paire ancienne choisie explicitement et refus contrôlé des paires non
supportées. Exiger payloads golden équivalents, replay sans publication double,
aucune réservation pending permanente après erreur, aucun snapshot partiel,
ressources bornées et recovery après restart. Les tests Runtime certifient les
primitives, pas le mapping externe. Ce document ne fournit pas l'adaptateur.
