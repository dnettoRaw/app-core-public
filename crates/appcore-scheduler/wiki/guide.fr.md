# appcore-scheduler

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** exécution locale bornée et placement Core explicable.

**Dépendances internes :** `appcore-contracts`, `appcore-core`.

**API principale :** `Scheduler`, `SchedulerConfig`, `ScheduledTask`,
`TaskSchedule`, callback/context/result, retry policy, handle et snapshots;
`DurableSchedulerConfigV1`, `SchedulerStateProvider`, providers mémoire et
fichier, claims et receipts V1;
requêtes/candidats/rejets/évaluations/décisions ressources et
`PlacementEngine`.

À utiliser pour travail local déclaré avec limites, annulation et shutdown. Ce
n'est ni workflow engine durable ni file distribuée.

Le shutdown ferme l'admission sous le lock d'état et l'arithmétique des
deadlines est vérifiée. Les temps one-shot, interval ou retry non
représentables renvoient `InvalidSchedule` ou retirent la task épuisée au lieu
de paniquer.

Le scheduler crée un pool fixe, limité par `max_concurrent_tasks`, et une file
bornée à deux fois ce nombre effectif de workers ou à `max_tasks`. Quand les
slots de dispatch et la file sont occupés, les tâches dues suivantes restent
dans le registre sans consommer de tentative. La pression est observable avec
`worker_thread_count`, `queued_task_count` et `queue_saturation_count`. Le
shutdown ferme l'admission et draine les callbacks déjà acceptés avec
`TaskContext::is_cancelled()` activé. Les callbacks ne sont ni terminés de
force ni soumis à un timeout préemptif, car les threads Rust ne peuvent pas
être interrompus en toute sécurité.

Le contrat d'état opt-in de la version candidate `1.0.2-rc` ne conserve que
l'identité task, le hash de définition, next run, attempts, policy misfire,
claim actuel, epoch de fencing et dernier receipt. Un receipt one-shot confirmé
empêche l'exécution après restart. Un claim expiré sans receipt produit une
récupération at-least-once : les effets callback doivent utiliser l'epoch
exposé ou leur propre frontière d'idempotency. Le provider de référence local
au processus prouve les claims bornés entre deux owners. Configurez
`Scheduler::with_state_provider`, puis enregistrez uniquement le travail choisi
avec `schedule_durable`; les appels `schedule` restent éphémères. Le provider
fichier persiste le contrat avec des locks locaux et interprocessus, un snapshot
V1 borné et checksummed et un remplacement atomique. Les callbacks doivent
appliquer `TaskContext::fencing_epoch` à la frontière de l'effet protégé quand
plusieurs owners sont possibles. Voir la
[décision V1](../../../release/scheduler-state-provider-v1.md).

**Maturité :** profil RC actuel ; état durable opt-in.
