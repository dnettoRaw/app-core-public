# appcore-transport

Tests locaux:

```bash
cargo test -p appcore-transport
```

**Responsabilité :** mécanique client HTTP/TLS partagée et bornée.

**Dépendances internes :** aucune.

Le crate possède un SemVer indépendant. Les adaptateurs d'infrastructure
peuvent l'utiliser sans l'hôte AppCore Runtime.

**API principale :** `HttpScheme`, `HttpTarget`, `HttpRequest`, `HttpHeader`,
`HttpClient`, `HttpExchangeConfig`, `HttpTimeouts`, `HttpPoolConfig`,
`HttpResponse`, `CancellationToken`, `TransportError`, `send`, parsing de
réponse et gzip borné.

Conservez et clonez un `HttpClient` afin de réutiliser les connexions HTTP/1.1
entièrement consommées. `HttpPoolConfig` borne les connexions actives, les
connexions inactives et les origines retenues. `HttpTimeouts` sépare les délais
de connexion/admission, de lecture et d'écriture. Une réponse tronquée,
malformée ou avec `Connection: close` ne revient jamais dans le pool. La
fonction `send` existante reste un adaptateur V1 one-shot et continue d'envoyer
`Connection: close`.

Les bodies de requête sont des octets immuables partagés. `HttpRequest::new`
conserve son contrat d'entrée `Vec` owned et le déplace vers le stockage
partagé ; `HttpRequest::from_shared_body` accepte un `Arc<[u8]>` existant.
Cloner une requête ou la transférer à un worker borné ne duplique donc pas un
grand body.

À utiliser dans les adapters partageant limites, timeout, annulation et TLS. Le
consommateur garde authentification et policy. Ne pas en faire un framework web
ni ajouter d'endpoints métier.

Le `Debug` request/response expose la taille du body, jamais ses bytes. Les
headers de credential connus sont masques meme si l'appelant utilise le
constructeur non sensible.

**Maturité :** surface infrastructure RC stable.

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

`parse_response_owned` prend possession du frame complet. Il compacte les corps
identity fixes et chunked dans la même allocation et les retourne sans seconde
copie. `HttpClient` et le `send` one-shot utilisent ce chemin. Les réponses
compressées gardent leur sortie de décodage bornée ; l'API empruntée
`parse_response` reste inchangée.

## Documentation stable

Identifiant stable : **ACR-004**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-004). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
