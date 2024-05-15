# appcore-ops

**Responsabilidade:** health, logs, metrics, observations, heartbeat e
availability sem dependência de vendor.

**Dependências internas:** `appcore-core`, `appcore-supervisor`.

**API principal:** health status/report/checks, heartbeat sources, loggers,
metric counters, `ObservationEvent`/`ObservationSink`, file sink limitado,
availability report e reexports de compatibilidade para
`appcore-supervisor::managed_services`.

Use para sinais operacionais genéricos. Código novo de lifecycle usa
`appcore-supervisor` diretamente. Não adicione SDK de vendor nem métricas de
negócio da aplicação ao crate.

**Maturidade:** primitives RC estáveis; export/collection de produção pertence
ao deployment.
