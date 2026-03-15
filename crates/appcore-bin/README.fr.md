# appcore-bin

**Responsabilité :** façade manifest-first, CLI et composition root.

**Dépendances internes :** tous les crates service/composition.

**API application :** `Application`, `run_application`,
`ManifestApplicationHost`, `ApplicationServiceReport`, `DeploymentContext`,
volumes/environment résolus et `ApplicationTaskRegistry`.

**API host :** bootstrap/config errors/results, CLI, paths/lifecycle local,
server entry points, build info et outils auth-server optionnels.

C'est la dépendance recommandée des applications. Il possède chargement des
manifests, providers, lifecycle, HTTP, sync, peer RPC, control plane,
Gateway, scheduling, supervision, updates et shutdown.

Les applications utilisent le module public `application` et évitent internals.

`appcore-bin` et `appcore-auth-server` utilisent tous deux la frontière bornée
de `appcore-args`. L'aide et les candidats de complétion proviennent de la même
spécification de commandes validée.

Les descripteurs capability finaux du manifeste sont composés une fois par
`appcore-capabilities`. La façade directe, le HTTP applicatif et le peer RPC
utilisent cet owner pour l'enforcement de mode, idempotence, mode d'écriture et
leadership.

Quand `deployment.toml` selectionne `[adapters.gateway]` avec le provider
`appcore-gateway`, le bootstrap valide la configuration owner, ajoute et
autorise `runtime.gateway` dans ce catalogue, reutilise la securite du Runtime
et enregistre l'instance dans le Supervisor. Une erreur de bind ou de
configuration arrete le startup. `ApplicationServiceReport` expose les champs
Gateway started/state/bind sans credentials. Le host fournit un replay store
durable et sur entre processus; cluster exige `paths.gateway_replay` absolu sur un
volume partage et inscriptible. Le shutdown ferme les connexions incompletes
avant son delai et joint le listener et la thread runtime.

```bash
appcore-bin completions zsh
appcore-auth-server completions powershell
```

**Maturité :** façade manifest-first RC stable; internals restent détails.
