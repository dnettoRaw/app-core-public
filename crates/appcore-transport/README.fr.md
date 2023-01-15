# appcore-transport

**Responsabilité :** mécanique client HTTP/TLS partagée et bornée.

**Dépendances internes :** aucune.

Le crate possède un SemVer indépendant. Les adaptateurs d'infrastructure
peuvent l'utiliser sans l'hôte AppCore Runtime.

**API principale :** `HttpScheme`, `HttpTarget`, `HttpRequest`, `HttpHeader`,
`HttpClientConfig`, `HttpResponse`, `CancellationToken`, `TransportError`,
`send`, parsing de réponse et gzip borné.

À utiliser dans les adapters partageant limites, timeout, annulation et TLS. Le
consommateur garde authentification et policy. Ne pas en faire un framework web
ni ajouter d'endpoints métier.

Le `Debug` request/response expose la taille du body, jamais ses bytes. Les
headers de credential connus sont masques meme si l'appelant utilise le
constructeur non sensible.

**Maturité :** surface infrastructure RC stable.
