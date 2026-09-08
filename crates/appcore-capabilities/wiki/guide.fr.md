# appcore-capabilities

`CapabilityCatalog` applique les mêmes limites de nombre et de taille de version.
`from_descriptors` arrête de consommer l'entrée au premier rejet, sans tout
collecter avant validation.

Le `CapabilityRegistry` local admet au plus 4 096 handlers et des versions de
descriptor de 1–256 octets UTF-8. Un rejet préserve les handlers et renvoie
`HandlerRejected` avec une raison bornée. `iter_descriptors` emprunte les
descriptors sans clonage ni collection allouée ; l'ordre est indéfini. Ces
limites ne contrôlent pas la mémoire interne des handlers externes ou de leur
méthode descriptor.

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

La sélection par défaut limite les allocations au résultat choisi : le
discovery est parcouru par références empruntées de peer et de descriptor,
seul le premier fallback compatible est gardé et seul le provider retenu est
cloné. La compatibilité utilise le descriptor déjà trouvé au lieu de parcourir
une copie de tous les noms annoncés. Une `CapabilitySelectionPolicy`
personnalisée conserve le comportement stable et reçoit le slice owned complet.

Lorsque le caller n'a plus besoin de la request, utilisez
`CapabilityResolver::handle_owned`. La résolution et la policy continuent à
emprunter la request ; un handler local sélectionné conserve le contrat
borrowed stable, tandis que l'invoker Peer RPC transfère tous les champs owned
vers son DTO sortant. `RemoteCapabilityInvoker::invoke_remote_owned` possède un
default borrowed, donc les invokers personnalisés existants restent compatibles.

Les trois méthodes d'exécution empruntent le provider local ou le record de
discovery sélectionné par défaut jusqu'à la fin du dispatch. Cela évite de
cloner l'identité, les endpoints, les capabilities et la metadata d'un peer
pour un appel transitoire. `resolve()` retourne toujours un provider owned,
tandis qu'un selector personnalisé conserve le contrat de la liste complète
des candidats owned.

**Maturité :** profil de routage RC stable.
