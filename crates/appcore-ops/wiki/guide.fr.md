# appcore-ops

[Exemple minimal](examples/basic.fr.md) |
[Exemple intermediaire](examples/intermediate.fr.md)

**Responsabilité :** health, logs, métriques, observations, heartbeat et
availability sans vendor.

**Dépendances internes :** `appcore-core`, `appcore-supervisor`.

**API principale :** health status/report/checks, heartbeat sources, loggers,
metric counters, `ObservationEvent`/`ObservationSink`, file sink borné,
availability report et reexports de compatibilité pour
`appcore-supervisor::managed_services`.

Le sink d'observations local au processus retient au plus 65 536 événements et
16 Mio, avec un plafond d'octets plus strict dérivé des petites politiques de
comptage. Le registre de métriques retient au plus 4 096 noms, 128 octets par
nom et 1 Mio agrégé. Tous deux exposent pression de comptage/octets et snapshots
immuables partagés ; les snapshots compatibles produisent encore des valeurs
owned. Une observation trop grande n'est pas retenue mais atteint toujours les
32 drains configurés au maximum. Le logger mémoire retient lui aussi au plus
4 096 enregistrements et 8 Mio et fournit `shared_records`.
La configuration des drains utilise une génération immuable copy-on-write.
`emit` partage cette génération avec un clone d'`Arc` au lieu de cloner jusqu'à
32 handles, libère le lock de configuration, puis appelle les drains.
`SharedObservationEvent::new` applique l'expurgation et les limites de champs
une seule fois. Le hub mémoire transmet ce payload immuable via
`ObservationSink::emit_shared` ; les sinks mémoire, fichier et métriques
redéfinissent la méthode sans clone profond. Les implémentations existantes ont
seulement besoin d'`emit` et utilisent automatiquement le fallback owned.
Les noms d'attributs sensibles utilisent un parcours des octets ASCII insensible
à la casse et sans allocation. Le matching conservateur par sous-chaîne reste
inchangé, sans créer de `String` en minuscules pour chaque attribut.

Le file sink borné valide à nouveau le nom, la trace, le nombre d'attributs,
les clés et les valeurs dans `emit`, y compris pour les événements construits
via les champs publics. Le worker mesure un enregistrement JSONL avec un
counting writer borné avant rotation, puis transmet ce même enregistrement au
disque. Il n'alloue jamais une copie sérialisée complète. Un enregistrement
plus grand que l'espace utile d'un fichier vide échoue fermé et apparaît dans
`FileObservationSinkStats::errors`, sans rotation. La file accepte au plus
65 536 éléments et retient au plus 8 Mio entre les enregistrements en attente
et en cours d'écriture. `FileObservationSink::pressure` expose les octets
actuels, le pic et les rejets dus au budget d'octets.

`FileObservationSink::flush` utilise le délai de 30 secondes
`FILE_OBSERVATION_FLUSH_TIMEOUT`. Son deadline unique commence avant
l'admission dans la file bornée et inclut l'acknowledgement durable du worker.
Appelez `flush_timeout(Duration::from_secs(...))` pour un délai positif plus
court. Une file pleine ou l'absence d'acknowledgement renvoie
`ErrorKind::TimedOut` ; une durée nulle ou en overflow renvoie `InvalidInput`.
Si la commande est déjà admise, le worker peut la terminer après le timeout du
caller.

À utiliser pour signaux génériques. Le nouveau code lifecycle utilise
`appcore-supervisor` directement. Ne pas ajouter de SDK vendor ni métriques
métier applicatives au crate.

**Maturité :** primitives RC stables; export/collection production appartient
au déploiement.

## Rétention des snapshots de métriques

Un snapshot partagé est immuable, mais le conserver pendant une mise à jour
oblige celle-ci à copier les nœuds du registre. Les noms restent partagés.
La pression du registre couvre uniquement la génération actuelle, pas les
snapshots des consommateurs. Borner les noms ne borne pas la mémoire du processus.

Le collecteur doit libérer le snapshot précédent avant d'obtenir le suivant,
ou utiliser une file explicitement bornée qui évince avant admission. Bornez
tous les consommateurs et clones, exports en cours compris. Ne remplacez pas
les valeurs du snapshot par des lectures atomiques vivantes : cela changerait
la sémantique temporelle.

La famille de benchmarks `metric_update_4096_retained_0/1/16` alterne snapshots
et mises à jour de 4 096 compteurs avec 0, 1 ou 16 générations conservées.
Fixture et remplissage initial sont hors chronométrage ; acquisition, éviction
et mise à jour sont mesurées. Le RSS inclut les générations préremplies ; le
point de rétention suit leur libération et peut inclure la mémoire conservée
par l'allocateur. Ces cas ne certifient pas le budget global d'un déploiement.
