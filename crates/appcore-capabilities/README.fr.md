# appcore-capabilities

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
leadership.

**Maturité :** profil de routage RC stable.
