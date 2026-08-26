# appcore-control-plane

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

À utiliser pour coordination distribuée sans payload métier. Le profil file
exige locks/storage certifiés. Le distant exige TLS et authentification du
déploiement.

Le profil fichier limite l'état et le backup à 16 MiB et rejette tout état
malformé ou futur. L'arithmétique d'expiration et d'epoch est vérifiée;
l'épuisement de l'epoch échoue fermé au lieu de réutiliser un fencing token.

**Maturité :** contrats et références RC stables; l'exploitation du service
externe appartient au déploiement.
