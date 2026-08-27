# appcore-scheduler

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

Les callbacks s'exécutent dans un pool fixe. Le pool ne dépasse jamais
`max_concurrent_tasks` et sa file interne est limitée au minimum entre deux
fois le nombre de workers et `max_tasks`. Le travail dû excédentaire reste
planifié sans consommer de retry; `queued_task_count` et
`queue_saturation_count` rendent la pression observable. Le shutdown draine
les callbacks acceptés avec l'annulation indiquée dans `TaskContext`; aucun
timeout préemptif non sûr n'est appliqué, les callbacks longs doivent donc
coopérer via `is_cancelled`.

La version candidate alpha 1.5 fournit la récupération opt-in via
`SchedulerStateProvider` V1. Démarrez avec `Scheduler::with_state_provider`,
puis utilisez `schedule_durable` seulement pour les tâches choisies. Le Runtime
persiste next run, attempts et receipts, renouvelle les claims bornés et expose
l'epoch monotone de fencing au callback. `FireOnce` et `Skip` sont explicites.
`Scheduler::new` et `schedule` restent locaux au processus et offline. Le
provider fichier utilise un snapshot V1 borné et checksummed, des locks locaux
et interprocessus, un remplacement atomique et le sync du répertoire. La
récupération reste at-least-once jusqu'au commit du receipt.

**Maturité :** profil local RC stable; scheduling local au processus.
