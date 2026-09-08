# appcore-supervisor

Les appels start/stop concurrents ou réentrants échouent immédiatement sans
exécuter un second callback de cycle de vie. Health ne retient pas cette barrière.

`CallbackManagedService` traite une erreur ou un panic d'arrêt comme une
libération incomplète : l'état devient définitivement `Orphaned` et les appels
start/stop suivants échouent fermés. Le Supervisor enregistre quarantaine et
intervention opérateur. Le callback d'arrêt du scheduler doit renvoyer une
erreur si `shutdown_with_timeout` renvoie `Ok(false)`, jamais un succès. Les
callbacks doivent respecter le délai ; l'adapter ne peut pas les interrompre.

**Responsabilité :** lifecycle avec dépendances, health, budget de restart et
shutdown des managed services appartenant au Runtime.

**Dépendances internes :** aucune.

Le crate possède un SemVer indépendant et peut superviser des services gérés
dans tout processus Rust; AppCore est un consommateur.

**API principale :** `ManagedService`, `ServiceDescriptor`,
`ServiceDependency`, `DependencyRequirement`, `Supervisor`,
`SupervisorWatchdog`, `RestartPolicy`, `RestartState`, `ServiceHealth`,
`ServiceActivationState`, `ServiceRuntimeState`, snapshots/evenements types et
adapters.

À utiliser dans la composition root pour Scheduler, Peer RPC, Control Plane,
Jobs, Update, Auth Server, Metrics, Observation, Sync, workers et queues. Ne
pas l'utiliser pour redemarrer son processus host. Reconcile planifie le
restart; un executeur borne realise le lifecycle et le watchdog verifie le
progres.

Il n'existe aucun second module Supervisor ni alias dans `appcore-ops`.

Les panics de callback, factory et health probe deviennent des états d'échec
contrôlés; un panic de restart n'arrête pas le worker borné. L'arithmétique des
timeouts et les compteurs pending sont vérifiés. Les files de commandes et de
résultats sont bornées; la contre-pression des résultats reste annulable et le
shutdown libère les entrées retenues. Le shutdown est coopératif: un callback
arbitraire qui ignore l'annulation ne peut pas être interrompu de force en
sécurité dans le processus.

**Maturite :** contrat stable en evolution avec evenements, file, workers,
budgets et diagnostic bornes; la supervision du processus reste externe.

## Documentation stable

Identifiant stable : **ACR-005**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-005). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
