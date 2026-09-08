# appcore-control-plane

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

Tests locaux:

```bash
cargo test -p appcore-control-plane
```

Les retries HTTP concernent uniquement les réponses 408, 429, 500, 502,
503 et 504 et les erreurs de transport, timeout ou offline. Les autres statuts
et erreurs sémantiques typées sont renvoyés immédiatement. Le premier backoff
respecte aussi `max_backoff_ms`. Le délai local total décrit ci-dessus ne
prouve pas qu'une écriture distante au résultat ambigu peut être répétée sans
risque.

**Responsabilité :** implémentations génériques présence, heartbeat, discovery
et leases.

**Dépendances internes :** contracts, core, distributed contracts et transport.

**API principale :** clients in-memory, file et offline; configuration HTTP,
retry policy et trait transport ; transports standard one-shot, pooled et
bearer ; coordinator et heartbeat policy ; guards leadership global/service ;
validation endpoint sûr.

`PooledHttpTransport` est le profil HTTP réutilisable sans authentification et
`BearerHttpTransport` réutilise également son client borné.
`StdHttpTransport` conserve le profil V1 one-shot.
`HttpControlPlaneClient` convertit chaque body encodé une seule fois en
`SharedHttpControlPlaneRequest` et l'emprunte pendant les retries bornés. Les
transports internes réutilisent la même allocation immuable ; les transports
externes qui implémentent uniquement la méthode owned originale conservent le
fallback compatible. Le `Debug` de la requête indique la longueur du body sans
exposer ses octets.

À utiliser pour coordination distribuée sans payload métier. Le profil file
exige locks/storage certifiés. Le distant exige TLS et authentification du
déploiement.

Le profil fichier limite l'état et le backup à 16 MiB et rejette tout état
malformé ou futur. L'arithmétique d'expiration et d'epoch est vérifiée;
l'épuisement de l'epoch échoue fermé au lieu de réutiliser un fencing token.

Le profil mémoire admet par défaut au maximum 65 536 inscriptions et slots de
lease combinés sous un budget estimé de 16 Mio retenus. Utilisez `with_limits`
pour réduire les deux limites et `stats` pour observer les octets actuels/de pic
et les rejets. Un remplacement trop volumineux conserve l'inscription ou le
lease précédent. Le profil fichier limite aussi l'état décodé à 262 144
enregistrements et 64 Mio, sans modifier la limite JSON V1 de 16 Mio.

Le JSON d'état est décodé par un reader borné et sérialisé directement dans un
fichier temporaire exclusif. Backup et restore transmettent un seul fichier
borné vers une génération staged, valident cette génération exacte, la
synchronisent puis remplacent la destination. Les maps décodées restent
l'unique owner complet de l'état; aucun second buffer JSON complet n'est gardé.

**Maturité :** contrats et références RC stables; l'exploitation du service
externe appartient au déploiement.

## Documentation stable

Identifiant stable : **ACR-015**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-015). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
