# appcore-control-plane

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

À utiliser pour coordination distribuée sans payload métier. Le profil file
exige locks/storage certifiés. Le distant exige TLS et authentification du
déploiement.

Le profil fichier limite l'état et le backup à 16 MiB et rejette tout état
malformé ou futur. L'arithmétique d'expiration et d'epoch est vérifiée;
l'épuisement de l'epoch échoue fermé au lieu de réutiliser un fencing token.

**Maturité :** contrats et références RC stables; l'exploitation du service
externe appartient au déploiement.
