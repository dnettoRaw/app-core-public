# appcore-capabilities

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** cataloguer les descripteurs composés, enregistrer les
handlers locaux et résoudre les providers locaux ou distants compatibles.

**Dépendances internes :** contracts, core et distributed contracts.

**API principale :** request/response/error, traits local handler et remote
invoker, catalogue et contexte d'enforcement, local provider, registry,
provider selection, resolution policy, selection trait/default, resolver et
invoker peer RPC fondé sur le contrat distribué.

Utiliser IDs génériques et exigences explicites. Le resolver considère health,
mode, leadership et policy; il n'interprète pas la sémantique produit.

Utilisez `CapabilityCatalog` lorsque la composition root doit résoudre et
autoriser les descripteurs du manifeste avant le dispatch. Utilisez
`CapabilityRegistry` uniquement avec un vrai handler local. Catalogue et
resolver partagent l'enforcement de request, mode d'écriture et leadership.

**Maturité :** profil de routage RC stable.
