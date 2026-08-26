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

## AI alpha optionnelle

La feature `ai-alpha` rattache un `appcore_ai::AiRuntime` déjà configuré au
Supervisor existant sans modifier les manifests V1 gelés :

```rust
let component = Arc::new(AppCoreAiComponent::new(Arc::new(ai_runtime), false)?);
let ai = component.facade();
let business = MonApplication::new(ai);
ManifestApplicationHost::load("application.toml", "deployment.toml", &business)?
    .with_ai(component)
    .run()?;
```

`required = true` fait échouer le démarrage si aucun modèle/backend n'est
utilisable ; `false` démarre en état dégradé. Le shutdown refuse les nouvelles
admissions, annule les requêtes actives et respecte le délai borné du
Supervisor. Exposer `appcore.ai.resolve` via `appcore-capabilities` exige un
`AiCapabilityCodec` borné appartenant à l'application ; les types Rust ne sont
pas un wire format implicite. La sélection déclarative exige un futur contrat
de manifest versionné post-1.0.

`appcore-bin` et `appcore-auth-server` utilisent tous deux la frontière bornée
de `appcore-args`. L'aide et les candidats de complétion proviennent de la même
spécification de commandes validée.

Les descripteurs capability finaux du manifeste sont composés une fois par
`appcore-capabilities`. La façade directe, le HTTP applicatif et le peer RPC
utilisent cet owner pour l'enforcement de mode, idempotence, mode d'écriture et
leadership.

Les handlers de commande de la façade directe, du HTTP applicatif et du peer
RPC s'exécutent sans conserver le mutex partagé du host. Les commandes
indépendantes progressent en parallèle ; réservation et finalisation
idempotentes restent sérialisées par store. Le shutdown refuse les nouvelles
admissions, draine pendant au plus 30 secondes les commandes admises, puis
termine le lifecycle. Les tests peuvent choisir une borne plus courte avec
`ManifestApplicationHost::shutdown_with_timeout`.
L'enregistrement des queries applicatives est gelé après le bootstrap ; les
queries directes, HTTP et peer RPC clonent le router immuable et s'exécutent
sans le mutex du host.

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
