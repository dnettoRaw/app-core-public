# appcore-control-plane

Release candidate **`appcore-control-plane 1.0.4-rc.4`**. Sélectionner explicitement cette RC ; elle ne remplace pas les versions stables antérieures et ne certifie pas un déploiement. Les dépendances AppCore internes, lorsqu’elles existent, sont fixées sur cette série RC.


Abandonner la future avant le dispatch empêche l'exécution de l'opération.
La file conserve une référence faible au résultat, sans retenir le waker ni la
réponse d'une future abandonnée. La closure et son slot restent jusqu'au retrait.
Après dispatch, abandonner la future n'annule pas les effets distants et
n'interrompt pas un transport synchrone ; utilisez l'annulation coopérative.

Acquisition/renouvellement et libération de lease utilisent un seul essai HTTP,
quelle que soit la configuration des retries. V1 ne fournit pas de clé de
déduplication distante ; la réponse peut se perdre après application. Timeout
ou erreur HTTP transitoire ne prouvent pas que le lease est inchangé. Réconciliez
l'état autoritatif et le fencing avant les écritures dépendant du leadership,
sans rejouer aveuglément. Discovery, enregistrement et heartbeat conservent
leurs retries configurés ; la sémantique distante exige des tests de conformité.

Les attentes utilisent un jitter non cryptographique entre la moitié (arrondie
vers le haut) et le plafond exponentiel courant, borné aussi au temps restant.
Un backoff nul reste nul. Chaque cycle initialise son état local avec le hasher
aléatoire de la bibliothèque standard ; ce n'est pas un aléa de sécurité.

Les retries HTTP sont bornés lors de l'exécution : au plus 16 tentatives,
un timeout de 1–30 000 ms et un backoff maximal de 30 000 ms. Zéro tentative
signifie toujours une ; le backoff nul reste permis. Le budget conservateur est
tentatives × timeout + (tentatives − 1) × backoff maximal, plafonné à 120 secondes.
Une horloge monotone borne tentatives et attentes au temps restant. Une réponse
tardive renvoie Timeout sans nouvel essai, même en cas de succès distant ;
l'opération peut déjà avoir été appliquée. Les transports externes doivent
respecter le délai : les callbacks synchrones ne sont pas interrompus de force.
Le budget commence à l'appel du provider et inclut file, encode et decode.
Un request expiré en file n'atteint pas le transport. L'expiration est observée
quand le worker avance, sans timer indépendant : un transport précédent bloqué
peut retarder la future.

Les retries HTTP concernent uniquement les réponses 408, 429, 500, 502,
503 et 504 et les erreurs de transport, timeout ou offline. Les autres statuts
et erreurs sémantiques typées sont renvoyés immédiatement. Le premier backoff
respecte aussi `max_backoff_ms`. Cette politique ne définit pas de délai total
et ne prouve pas qu'une écriture au résultat ambigu peut être répétée sans risque.

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** implémentations génériques présence, heartbeat, discovery
et leases.

**Dépendances internes :** contracts, core, distributed contracts et transport.

**API principale :** clients in-memory, file et offline; configuration HTTP,
retry policy et trait transport ; transports standard one-shot, pooled et
bearer ; coordinator et heartbeat policy ; guards leadership global/service ;
validation endpoint sûr.

Utilisez `PooledHttpTransport` pour les appels réutilisables sans
authentification. `BearerHttpTransport` possède aussi un client réutilisable et
borné. Conservez `StdHttpTransport` uniquement lorsque le comportement V1
one-shot avec `Connection: close` est requis.
`HttpControlPlaneClient` convertit un body encodé une seule fois en
`SharedHttpControlPlaneRequest` et l'emprunte pour tous les retries bornés. Les
transports internes clonent uniquement son owner partagé ; les transports
externes existants qui implémentent la méthode owned utilisent encore le
comportement compatible par défaut. Les deux owners de requête omettent les
octets du body dans leur sortie `Debug`.

À utiliser pour coordination distribuée sans payload métier. Le profil file
exige locks/storage certifiés. Le distant exige TLS et authentification du
déploiement.

Le profil fichier limite l'état et le backup à 16 MiB et rejette tout état
malformé ou futur. L'arithmétique d'expiration et d'epoch est vérifiée;
l'épuisement de l'epoch échoue fermé au lieu de réutiliser un fencing token.

`InMemoryControlPlane` utilise par défaut 65 536 inscriptions/slots de lease
combinés et un budget estimé de 16 Mio retenus. `with_limits` peut réduire les
deux ; `stats` expose les octets actuels/de pic, les comptes et les admissions
rejetées. Le rejet est atomique : un enregistrement existant reste utilisable.
L'état fichier conserve la frontière JSON V1 de 16 Mio et limite aussi l'état
décodé à 262 144 enregistrements et 64 Mio.

Son JSON V1 est décodé par un reader borné et écrit directement dans un fichier
temporaire exclusif. Backup et restore copient un stream borné, valident la
génération staged exacte, la synchronisent puis remplacent la destination.
Seules les maps décodées gardent l'état complet en mémoire; la persistance ne
retient pas un second buffer JSON complet.

**Maturité :** contrats et références RC stables; l'exploitation du service
externe appartient au déploiement.

## Candidat de publication — 2026-10-14

`1.0.4-rc.2`: alignement des dépendances.


## Correction en source : restauration préservant le fencing

`FileControlPlane::restore_from` exige désormais un état courant lisible et exactement les mêmes slots de leases dans la sauvegarde préparée (y compris epochs retenus, holder et timestamps de renouvellement/expiration). Toute différence retourne `ControlPlaneError::Conflict` avant remplacement. Un état courant corrompu produit une erreur contrôlée sans changer ses octets. Copie, validation et remplacement restent sous verrou exclusif ; le temporaire rejeté est supprimé. La présence reste restaurable avec un fencing identique.

Migration : JSON V1 et signatures Rust inchangés, récupération volontairement plus stricte. Une ancienne sauvegarde après acquisition, renouvellement, expiration/réacquisition ou release est refusée ; cette API ne répare plus un état courant corrompu dont les epochs utilisés ne peuvent être prouvés. Conserver état et sauvegarde pour examen. Ne jamais supprimer/réinitialiser le répertoire ni revenir au comportement permissif pour contourner le refus. Une perte totale de l’arbitre ou un rollback externe des fichiers exige une procédure de récupération/domaine distincte, non fournie ici. Correction locale non publiée ; adoption après nouvelle version immuable. Elle n’ajoute ni authentification Gateway, ni promotion distribuée, ni protection contre restauration hors API.

Qualification : 45 tests ciblés du crate passent sur macOS : reproduction du rollback d’epoch auparavant échouée, tombstones après release, renouvellement/holder de même epoch, présence avec fences identiques, corruption préservée, restart et course entre handles indépendants. Cela ne qualifie pas deux machines physiques, DNET ou Windows.

La restauration retient temporairement les états courant et préparé décodés : chacun est admis jusqu’à 262144 entrées et 64 MiB de payload retenu estimé (128 MiB ensemble), plus buffers bornés et overhead d’allocation. Aucune copie JSON encodée complète n’est conservée.


## Autorité exacte par service — source non publiée

`ServiceLeaseAuthority` est un contrat additif dans `appcore-distributed-contracts::control_plane::lease_authority`. Acquisition, renouvellement exact, libération exacte, inspection et check utilisent l’horloge serveur. `FileControlPlane` impose TTL 1–30000 ms : une acquisition du même Core retourne le grant vivant sans prolongation ; renew ne réacquiert pas une lease expirée et refuse les grants modifiés. Écriture des données et coordination des updates utilisent des `ServiceId` distincts choisis par le déploiement. Heartbeat, rôle et epochs HA Gateway ne confèrent aucune autorité sur les données.

Au restart utiliser `FileControlPlane::open_existing_with_clock` : état absent/corrompu refusé, aucun nouveau domaine d’epochs créé. Le provisionnement initial via `with_clock` est explicite. Les opérations utilisent un verrou OS sans attente et signalent contention/indisponibilité. `with_fenced_service_write` vérifie le grant exact et exécute un commit local fiable et borné sous le même verrou ; un takeover ne peut chevaucher ce commit même si la lease expire pendant celui-ci. Le callback ne doit pas réentrer dans l’arbitre ; le host possède bornes I/O et idempotence applicative. Un check distant reste une observation : le storage distant doit appliquer le fencing atomiquement au commit. Aucun check mémorisé n’autorise une écriture offline.

Gateway ajoute la composition explicite `make_service_lease_router(gateway, authority, replay)`, absente du router default. POST `/v1/gateway/service-lease` accepte `ServiceLeaseRequestV1` (version=1, identity, service_id, action) ; actions acquire/renew/release/inspect/check. Authentification obligatoire même en insecure-testing. Le domaine isolé `service_lease_token_claims` exige nom command/query exact, subject Core, hash complet du request et JTI unique jusqu’à 96 octets. La durée du credential ne dépasse ni le TTL demandé à l’acquisition ni l’expiration du grant pour les actions liées. Tokens relay, scope/action/epoch modifiés et wildcards sont refusés. L’émetteur autorise tenant/cluster/Core/service/action avant signature ; les clients ne reçoivent pas le secret de signature.

Composer un `PeerNonceStore` durable dédié partagé entre restarts/répliques de l’arbitre, par exemple `FilePeerNonceStore` sur stockage certifié. Aucun default implicite. Requests limités à 16 KiB et huit opérations sans file d’admission. Authentification/replay I/O et appels provider s’exécutent sur workers bloquants hors du polling async. Le déploiement possède deadlines I/O, drainage, TLS, horloge partagée et conservation des états. Réponses : 401 sans credential, 403 invalide/rejoué, 409 grant obsolète/conflit, 503 autorité indisponible/en contention ou saturation. Aucun succès offline. Échec provider ou réponse perdue reste ambigu : inspecter avec un nouveau credential sans deviner le succès ni rejouer une écriture avec un nouvel epoch.

Limites : Core est l’identité d’écriture, pas le PID ; partager ses credentials partage son autorité. Provisionner des Cores distincts pour les writers concurrents et un service logique stable pour la découverte. Référence sans consensus, sans arbitre HA à disques indépendants et sans fencing de storage distant. Perte/rollback total de l’état ou du replay nécessite récupération contrôlée/nouveau domaine ; supprimer les fichiers ou utiliser le constructeur initialisant n’est pas une récupération. Wire/storage V1 et APIs existantes préservés. Intégration consommateur, publication registry et qualification DNET/partition physique restent à faire.
