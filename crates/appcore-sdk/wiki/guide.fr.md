# Guide AppCore SDK

Le benchmark `runtime` mesure construction par défaut, préparation vide et
préparation de 32 hooks Core : commandes, événements, états, déclarations de
décision, nœuds de décision exécutables et handlers. Lancez
`cargo bench -p appcore-sdk --bench runtime`.
`APPCORE_BENCH_CASE` sélectionne `construct_defaults`, `prepare_empty` ou
`prepare_32_core_hooks`. Il mesure construction, validation et destruction,
pas le démarrage HTTP ni le cluster. Les queries et tasks dépendent de features
et ne font pas partie de ce cas Core. `--all-features` ajoute
`prepare_32_full_hooks`, qui prépare aussi 32 queries gelées et 32 définitions
de tasks bornées. Aucun backend optionnel n'est démarré.

`appcore-sdk` est la façade destinée au code applicatif. Commencez avec `App`
et `run`; ajoutez des manifestes explicites pour définir l'identité/version
locale ou la politique de déploiement.

1. Ajoutez `appcore-sdk` aux dépendances de l'application.
2. Appelez `appcore_sdk::run` avec un nom d'application stable.
3. Utilisez le `App` fourni uniquement pour le journal local et les manifestes validés.
4. Implémentez `Application` pour les commandes et les événements, puis appelez
   `App::prepare` avec l'identité explicite du node.
5. Ajoutez `api` pour les requêtes HTTP, `scheduler` pour les tâches planifiées
   et `deployment` seulement lorsque l'application consomme des bindings résolus par provider.
6. Déployez-la avec un processus Runtime explicite.

Le SDK ne démarre pas de listener, n'écrit pas de données, ne résout pas les
secrets et n'active aucun provider implicitement. Ses manifestes par défaut sont
des contrats V1 standalone, pas un second langage de configuration.

`App::prepare` est le pont de lifecycle avant le hosting. Il exécute les hooks
une seule fois, construit les registres Core immuables, fige le routeur de
queries et collecte les tâches bornées du scheduler. Le déploiement consomme
ces valeurs et reste propriétaire des providers, workers, de la cancellation
et du shutdown. Avec la feature `deployment`, il appelle
`Application::configure` seulement après avoir résolu et validé les bindings.

## Valeurs par défaut et manifestes explicites

Un programme local minimal nécessite une dépendance AppCore directe,
`appcore-sdk`, et aucun fichier manifeste. `App::new` construit néanmoins
des valeurs validées `ApplicationManifestV1` et `DeploymentManifestV1`.
Le déploiement standalone déclare le stockage `file` et les transports `http`,
mais ne les démarre pas.

1. Construisez `App` avec l'identité stable de l'application.
2. Appelez éventuellement `App::application_manifest` pour remplacer tout le
   contrat applicatif, sans fusionner certains champs avec les valeurs par défaut.
3. Appelez éventuellement `App::deployment_manifest` pour remplacer toute la
   politique d'installation. Les providers/réseaux explicites n'héritent pas
   des valeurs par défaut du SDK.
4. Les deux manifestes doivent correspondre à l'identité du `App`. Un contrat
   invalide ou une identité différente renvoie une erreur, jamais masquée par
   les valeurs par défaut.
5. Consultez `effective_application_manifest()` et
   `effective_deployment_manifest()`, puis appelez `run` ou `prepare`.

Les manifestes explicites permettent aussi de modifier la version, le service
et l'identité d'une installation locale ; ils n'exigent pas de services hébergés.
L'intégration de déploiement possède la lecture des fichiers et la résolution
des providers. Le SDK ne découvre ni ne charge automatiquement de fichiers
manifestes.

| Besoin | Surface du SDK |
|---|---|
| Callback local et journal | `run`, `App`, `AppResult`, `App::logger` |
| Comportement enregistré | `Application`, `NodeId` explicite, `PreparedApplication` |
| Requêtes / tâches | Features `api` / `scheduler` et registres préparés |
| Stockage / réplication | Namespaces et features `storage` / `sync` |
| AI / documents | Namespaces et features `ai` / `filemaker` |
| Bindings d'installation résolus | Feature `deployment` ; `prepare_with_deployment` |
| Cycle de vie du processus | Déploiement applicatif, pas un host du SDK |

Activer une feature expose des API, pas des services en cours d'exécution.
Identité du processus, providers, annulation et arrêt restent des responsabilités
explicites du déploiement.

Consultez [base](examples/basic.fr.md) et [intermédiaire](examples/intermediate.fr.md).
Le catalogue exécutable suit la même progression : `01-basics`,
`02-manifests`, `03-application`, `04-storage`, `05-sync`, `06-ai` et
`07-filemaker`.
`08-logging` et `09-api` complètent le catalogue destiné à l'application.

Le SDK par défaut ne dépend ni de l'API, ni d'un provider, ni du scheduler.
`storage`, `sync`, `ai` et `filemaker` sont aussi opt-in ; utilisez `full`
seulement pour un consommateur de développement qui requiert volontairement
toutes les capabilities du SDK.

Le log se configure une fois avec `App::logging(LoggerConfig)`. Choisissez le
terminal, le fichier, les deux, aucun log ou seulement les crashs. Le nom, la
taille, les rotations proches et l'archive optionnelle bornée en `YYYY/MM`
restent explicites. Le mode crash écrit son diagnostic mémoire borné avec
`App::dump_crash_log`.
