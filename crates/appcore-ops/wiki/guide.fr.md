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

À utiliser pour signaux génériques. Le nouveau code lifecycle utilise
`appcore-supervisor` directement. Ne pas ajouter de SDK vendor ni métriques
métier applicatives au crate.

**Maturité :** primitives RC stables; export/collection production appartient
au déploiement.
