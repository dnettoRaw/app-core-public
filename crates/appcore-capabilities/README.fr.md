# appcore-capabilities

Tests locaux:

```bash
cargo test -p appcore-capabilities
```

`CapabilityCatalog` applique les mêmes limites de nombre et de taille de version.
`from_descriptors` arrête de consommer l'entrée au premier rejet, sans tout
collecter avant validation.

Le `CapabilityRegistry` local admet au plus 4 096 handlers et des versions de
descriptor de 1–256 octets UTF-8. Un rejet préserve les handlers et renvoie
`HandlerRejected` avec une raison bornée. `iter_descriptors` emprunte les
descriptors sans clonage ni collection allouée ; l'ordre est indéfini. Ces
limites ne contrôlent pas la mémoire interne des handlers externes ou de leur
méthode descriptor.

**Responsabilité :** cataloguer les descripteurs, enregistrer les handlers
locaux et résoudre les providers locaux ou distants compatibles.

**Dépendances internes :** contracts, core et distributed contracts.

**API principale :** catalogue et contexte d'enforcement,
request/response/error, traits local handler et remote invoker, local provider,
registry, provider selection, resolution policy, selection trait/default,
resolver et invoker peer RPC fondé sur le contrat distribué.

Le catalogue valide les descripteurs composés du manifeste sans déclarer un
handler fictif; le registry ne contient que des handlers exécutables. Catalogue
et resolver partagent l'enforcement de mode, idempotence, écriture et
leadership. Le Runtime ne déduit aucune signification produit des noms de
capabilities.

Le resolver par défaut parcourt les records de discovery par emprunt, ne garde
que le premier fallback compatible et clone uniquement le provider retenu. Il
ne matérialise pas tous les peers compatibles et ne clone pas toute la liste
des capability names après que le descriptor a déjà correspondu. Une
`CapabilitySelectionPolicy` personnalisée continue de recevoir le slice owned
complet exigé par le contrat public stable.

Utilisez `CapabilityResolver::handle_owned` lorsque le caller possède une
request sélectionnée pour une exécution locale ou distante. Les handlers
locaux conservent leur contrat emprunté ; l'invoker Peer RPC déplace l'ID, la
capability, le payload, la clé d'idempotence et la trace directement dans la
request sortante. `handle` et `RemoteCapabilityInvoker::invoke_remote` restent
compatibles avec les callers et implémentations empruntés.

L'exécution par défaut de `handle`, `handle_local` et `handle_owned` emprunte
aussi le provider du registry ou le record de discovery sélectionné pendant
l'enforcement et le dispatch. `resolve()` continue délibérément à retourner un
`CapabilityProvider` owned, et les selectors personnalisés continuent à
recevoir leur liste owned complète de candidats.

**Maturité :** profil de routage RC stable.

## Documentation stable

Identifiant stable : **ACR-016**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-016). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
