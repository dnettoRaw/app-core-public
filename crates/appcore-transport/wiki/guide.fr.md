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

`HttpRequest` stocke son body dans des octets immuables partagés. Le
constructeur compatible `new` déplace un `Vec<u8>` owned dans ce stockage,
tandis que `from_shared_body` réutilise un `Arc<[u8]>` du caller. Les clones de
la requête partagent la même allocation, notamment lorsqu'un worker transport
borné doit posséder la requête après la suspension de la future appelante.

À utiliser dans les adapters partageant limites, timeout, annulation et TLS. Le
consommateur garde authentification et policy. Ne pas en faire un framework web
ni ajouter d'endpoints métier.

Le `Debug` request/response expose la taille du body, jamais ses bytes. Les
headers de credential connus sont masques meme si l'appelant utilise le
constructeur non sensible.

`encode_gzip_if_smaller` cesse de retenir le candidat gzip dès que les octets
produits atteindraient la taille d'entrée et renvoie `None`. Les demandes de
croissance du buffer ne dépassent pas `input.len() - 1` ; une entrée vide
renvoie `None` sans créer le codec. Cela ne borne ni mémoire interne du codec,
ni surcoût de l'allocateur, ni entrée, ni CPU déjà consommé avant émission.
Les candidats utiles gardent les mêmes octets gzip ; aucune heuristique
n'écarte une entrée potentiellement compressible.

Pour les réponses gzip non chunked, le parser emprunte les octets comprimés
de l'entrée pendant le décodage, évitant une seconde allocation du corps
comprimé. Le corps retourné reste owned. Le gzip chunked est d'abord compacté
dans la même allocation mais exige encore une sortie décompressée ; ce n'est
pas du streaming.

Lorsque le frame complet est déjà owned, `parse_response_owned` réutilise cette
allocation pour les corps identity fixes et chunked. Les clients intégrés
emploient ce chemin ; les callers empruntés gardent `parse_response`. Le
décodage compressé reste borné mais exige toujours une sortie owned.

**Maturité :** surface infrastructure RC stable.
