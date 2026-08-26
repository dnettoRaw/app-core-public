# appcore-scheduler

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** exécution locale bornée et placement Core explicable.

**Dépendances internes :** `appcore-contracts`, `appcore-core`.

**API principale :** `Scheduler`, `SchedulerConfig`, `ScheduledTask`,
`TaskSchedule`, callback/context/result, retry policy, handle et snapshots;
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

**Maturité :** profil local RC stable; scheduling local au processus.
