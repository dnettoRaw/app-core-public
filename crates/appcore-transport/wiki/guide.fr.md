# appcore-transport

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** mécanique client HTTP/TLS partagée et bornée.

**Dépendances internes :** aucune.

**Versionnement :** SemVer indépendant. Le crate peut être utilisé sans aucun
autre paquet AppCore.

**API principale :** `HttpScheme`, `HttpTarget`, `HttpRequest`, `HttpHeader`,
`HttpClient`, `HttpExchangeConfig`, `HttpTimeouts`, `HttpPoolConfig`,
`HttpClientConfig`, `HttpResponse`, `CancellationToken`, `TransportError`,
`send`, parsing de réponse et gzip borné.

Un `HttpClient` possède un pool borné par schéma, hôte et port. Ses clones
partagent ce pool. L'admission est bornée par origine, l'attente respecte le
délai de connexion et l'annulation, et les origines comme les sockets inactifs
sont bornés et expirent. Seule une réponse entièrement cadrée et analysée rend
le socket réutilisable. Troncature, cadrage invalide, timeout, annulation,
`Connection: close` et corps délimité par fermeture éliminent le socket.

Utilisez `HttpExchangeConfig` et `HttpTimeouts` pour séparer les délais de
connexion/admission, de lecture et d'écriture. `HttpClientConfig` et la fonction
libre `send` conservent le contrat V1 one-shot, y compris `Connection: close` ;
aucun consommateur existant n'active silencieusement le pooling.

À utiliser dans les adapters partageant limites, timeout, annulation et TLS. Le
consommateur garde authentification et policy. Ne pas en faire un framework web
ni ajouter d'endpoints métier.

Le `Debug` request/response expose la taille du body, jamais ses bytes. Les
headers de credential connus sont masques meme si l'appelant utilise le
constructeur non sensible.

**Maturité :** surface infrastructure RC stable.
