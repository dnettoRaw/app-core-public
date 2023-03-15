# appcore-supervisor

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** lifecycle avec dépendances, health, budget de restart et
shutdown des managed services appartenant au Runtime.

**Dépendances internes :** aucune.

**Versionnement :** SemVer indépendant. Le crate peut être utilisé sans aucun
autre paquet AppCore.

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
timeouts et les compteurs pending sont vérifiés. Le shutdown est coopératif:
un callback arbitraire qui ignore l'annulation ne peut pas être interrompu de
force en sécurité dans le processus.

**Maturite :** contrat stable en evolution avec evenements, file, workers,
budgets et diagnostic bornes; la supervision du processus reste externe.
