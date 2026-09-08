# appcore-supervisor

Les appels start/stop concurrents ou réentrants échouent immédiatement sans
exécuter un second callback de cycle de vie. Health ne retient pas cette barrière.

`CallbackManagedService` traite une erreur ou un panic d'arrêt comme une
libération incomplète : l'état devient définitivement `Orphaned` et les appels
start/stop suivants échouent fermés. Le Supervisor enregistre quarantaine et
intervention opérateur. Le callback d'arrêt du scheduler doit renvoyer une
erreur si `shutdown_with_timeout` renvoie `Ok(false)`, jamais un succès. Les
callbacks doivent respecter le délai ; l'adapter ne peut pas les interrompre.

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
timeouts et les compteurs pending sont vérifiés. Les commandes et résultats de
restart utilisent des files bornées distinctes. Un reconcile arrêté applique
une contre-pression annulable; le shutdown ferme l'admission et libère les
entrées retenues. Le shutdown reste coopératif: un callback arbitraire qui
ignore l'annulation ne peut pas être interrompu de force en sécurité dans le
processus.

**Maturite :** contrat stable en evolution avec evenements, file, workers,
budgets et diagnostic bornes; la supervision du processus reste externe.
