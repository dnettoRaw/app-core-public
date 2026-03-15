# appcore-bin

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** façade manifest-first, CLI et composition root.

**Dépendances internes :** tous les crates service/composition.

**API application :** `Application`, `run_application`,
`ManifestApplicationHost`, `ApplicationServiceReport`, `DeploymentContext`,
volumes/environment résolus et `ApplicationTaskRegistry`.

**API host :** bootstrap/config errors/results, CLI, paths/lifecycle local,
server entry points, build info et outils auth-server optionnels.

Les deux binaires traitent une entrée UTF-8 bornée avec `appcore-args`. L'aide,
la validation et la complétion dynamique Bash, Zsh, Fish et PowerShell
partagent une spécification déclarative; l'exécution reste dans ce crate.

Le manifeste distribué final alimente un catalogue unique
`appcore-capabilities` pendant le bootstrap. La façade directe, le HTTP
applicatif et le peer RPC utilisent le même owner pour l'enforcement de
déclaration, mode, idempotence, écriture opérationnelle et leadership. Les
queries de statut Runtime restent un comportement explicite du host.

Selectionner `[adapters.gateway]` avec le provider `appcore-gateway` est la
frontiere declarative d'activation du Gateway. Le bootstrap parse la
configuration dans la crate owner, ajoute et autorise `runtime.gateway` dans le
catalogue partage, reutilise la securite du Runtime et enregistre le service
dans le Supervisor. Une erreur de configuration ou de bind arrete le startup;
l'absence ne cree aucun listener ni task Gateway. `ApplicationServiceReport`
expose les champs surs started, state et bind, et le shutdown du host joint
tout le travail possede par le Gateway. Le replay store est sur entre
processus; cluster exige `paths.gateway_replay` absolu sur un volume partage et
inscriptible. Le shutdown ferme les connexions incompletes avant son delai.

C'est la dépendance recommandée des applications. Il possède chargement des
manifests, providers, lifecycle, HTTP, sync, peer RPC, control plane,
Gateway, scheduling, supervision, updates et shutdown.

Les applications utilisent le module public `application` et évitent internals.

**Maturité :** façade manifest-first RC stable; internals restent détails.
