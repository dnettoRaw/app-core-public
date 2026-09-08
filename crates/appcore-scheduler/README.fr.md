# appcore-scheduler

`shutdown_with_timeout(duration)` ferme l'admission et demande l'annulation
coopérative. `Ok(true)` signifie que coordinateur et workers sont terminés ;
`Ok(false)` signifie qu'ils restent actifs et ne doivent pas être remplacés.
L'appel peut être répété pour observer la fin. `shutdown()` utilise un budget
d'attente de cinq secondes et renvoie `SchedulerError::Shutdown` si incomplet.
`Drop` demande l'arrêt sans attendre et détache les threads encore actifs ;
mémoire et effets externes peuvent subsister jusqu'au retour des callbacks/providers.
Ce n'est ni une terminaison forcée ni un délai temps réel strict. Le deployment
doit mettre le scheduler incomplet en quarantaine et isoler les callbacks non fiables.

Tests locaux:

```bash
cargo test -p appcore-scheduler
```

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

La configuration rejette plus de `MAX_SCHEDULER_WORKERS` (64) threads callback
ou `MAX_SCHEDULER_TASKS` (65 536) tâches enregistrées. Coordinator et workers
utilisent des stacks explicites de 1 Mio.

Chaque parcours des tâches dues ne conserve que les meilleurs candidats qui
tiennent dans les slots de dispatch disponibles. Le max-heap borné préserve la
priorité décroissante, l'échéance la plus proche et l'ordre d'enregistrement,
et ne clone que les identifiants retenus. Au maximum configuré, 65 536 tâches
dues ne conservent donc pas plus de 128 candidats au lieu de matérialiser
l'ensemble complet.

La version candidate `1.0.2-rc` fournit la récupération opt-in via
`SchedulerStateProvider` V1. Démarrez avec `Scheduler::with_state_provider`,
puis utilisez `schedule_durable` seulement pour les tâches choisies. Le Runtime
persiste next run, attempts et receipts, renouvelle les claims bornés et expose
l'epoch monotone de fencing au callback. `FireOnce` et `Skip` sont explicites.
`Scheduler::new` et `schedule` restent locaux au processus et offline. Le
provider fichier utilise un snapshot V1 borné et checksummed, des locks locaux
et interprocessus, un remplacement atomique et le sync du répertoire. La
récupération reste at-least-once jusqu'au commit du receipt.

Le provider fichier décode avec un reader limité à 4 Mio. Save et checksum
empruntent les records récupérés, calculent le hash progressivement et
sérialisent directement avec un buffer fixe de 64 Kio dans le fichier
temporaire exclusif. Les octets V1 restent exacts sans conserver en même temps
le buffer fichier, une seconde liste DTO et une copie JSON encodée.

La validation du chargement emprunte aussi les champs task, definition, owner
et claim et vérifie l'ordre contre le dernier record converti. Un snapshot
maximal sans claims évite ainsi 3 072 allocations temporaires de chaînes sans
modifier les contrôles ni les octets V1.

**Maturité :** profil local RC stable; scheduling local au processus.

## Documentation stable

Identifiant stable : **ACR-014**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-014). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
