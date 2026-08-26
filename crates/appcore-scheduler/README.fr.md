# appcore-scheduler

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

Les callbacks s'exécutent dans un pool fixe. Le pool ne dépasse jamais
`max_concurrent_tasks` et sa file interne est limitée au minimum entre deux
fois le nombre de workers et `max_tasks`. Le travail dû excédentaire reste
planifié sans consommer de retry; `queued_task_count` et
`queue_saturation_count` rendent la pression observable. Le shutdown draine
les callbacks acceptés avec l'annulation indiquée dans `TaskContext`; aucun
timeout préemptif non sûr n'est appliqué, les callbacks longs doivent donc
coopérer via `is_cancelled`.

**Maturité :** profil local RC stable; scheduling local au processus.
