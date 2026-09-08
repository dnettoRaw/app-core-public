# appcore-ops

Tests locaux:

```bash
cargo test -p appcore-ops
```

**Responsabilité :** health, logs, métriques, observations, heartbeat et
availability sans vendor.

**Dépendances internes :** `appcore-core`, `appcore-supervisor`.

**API principale :** health status/report/checks, heartbeat sources, loggers,
metric counters, `ObservationEvent`/`ObservationSink`, file sink borné,
availability report et reexports de compatibilité pour
`appcore-supervisor::managed_services`.

`InMemoryObservationSink` borne les entrées et les octets retenus estimés, ne
préalloue jamais depuis une capacité non fiable et fournit des vues immuables
`ObservationSnapshot`. `InMemoryMetrics` borne aussi longueur des noms,
cardinalité et octets ; les noms rejetés restent visibles dans les compteurs de
pression. Les méthodes `snapshot` compatibles renvoient encore des valeurs
owned, tandis que `shared_snapshot` évite de cloner noms et événements retenus.
`InMemoryLogger` applique le même modèle à 4 096 enregistrements et 8 Mio ; sa
vue `shared_records` évite de cloner le texte de log expurgé.
La configuration des drains est une génération immuable copy-on-write. Chaque
observation partage cette génération avec un seul clone d'`Arc` au lieu de
cloner jusqu'à 32 handles de drain, et chaque callback s'exécute toujours après
la libération du lock de configuration.
`SharedObservationEvent::new` applique l'expurgation et les limites une seule
fois. Les sinks mémoire, fichier et métriques redéfinissent
`ObservationSink::emit_shared` afin que tous les drains retiennent ou inspectent
un seul payload immuable ; l'implémentation par défaut préserve les sinks
existants qui acceptent uniquement une valeur owned.
Les clés d'attribut sensibles sont examinées par un parcours des octets ASCII
insensible à la casse et sans allocation. La politique conservatrice existante
sur les sous-chaînes reste inchangée, sans allouer une `String` en minuscules
pour chaque attribut.

Le worker d'observations fichier revalide les champs publics de l'événement
avant l'admission dans la file bornée. Chaque enregistrement JSONL est compté
par un writer borné puis sérialisé directement dans le fichier actif, sans
retenir un second buffer JSON complet. Un enregistrement qui ne tient pas à
côté du header V1 dans un fichier vide est rejeté et incrémente
`FileObservationSinkStats::errors` ; il ne crée jamais une rotation hors limite.
L'admission limite aussi la file à 65 536 éléments et à 8 Mio pour les
enregistrements en attente ou en cours d'écriture. La méthode
`FileObservationSink::pressure` expose les octets actuels, le pic et les rejets
dus au budget d'octets.
`flush` applique `FILE_OBSERVATION_FLUSH_TIMEOUT`, soit 30 secondes, à
l'admission dans la file bornée et à l'acknowledgement du worker. Utilisez
`flush_timeout` pour un délai opérationnel positif plus court ; saturation ou
worker bloqué renvoie `TimedOut`. Un flush déjà admis peut finir après le
timeout du caller, sans dupliquer ni annuler de force l'I/O filesystem.

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

## Documentation stable

Identifiant stable : **ACR-013**. Consultez le
[guide complémentaire d’architecture et d’intégration](https://wiki.appcore.dnettoraw.com/fr/crates/id/acr-013). Cet identifiant
permanent reste valable si la page du wiki est déplacée.
