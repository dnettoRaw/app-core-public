# appcore-supervisor

**Responsabilidade:** lifecycle com dependências, health, orçamento de restart
e shutdown dos managed services pertencentes ao Runtime.

**Dependências internas:** nenhuma.

O crate possui SemVer independente e pode supervisionar managed services em
qualquer processo Rust; o AppCore é um consumidor.

**API principal:** `ManagedService`, `ServiceDescriptor`, `ServiceDependency`,
`DependencyRequirement`, `Supervisor`, `SupervisorWatchdog`, `RestartPolicy`,
`RestartState`, `ServiceHealth`, `ServiceActivationState`,
`ServiceRuntimeState`, snapshots/eventos tipados e adapters.

Use na composition root para Scheduler, Peer RPC, Control Plane, Jobs, Update,
Auth Server, Metrics, Observation, Sync, workers e queues. Nao use para
reiniciar o processo host. Reconcile apenas agenda restart; um executor
limitado executa o lifecycle e o watchdog independente verifica progresso.

Não existe um segundo módulo Supervisor nem aliases em `appcore-ops`.

Panics de callback, factory e health probe tornam-se estados de falha
controlados; um panic em um restart não encerra o worker limitado. Aritmética
de timeout e contadores pending são verificados. O shutdown é cooperativo, logo
um callback arbitrário que ignore cancelamento não pode ser interrompido à
força com segurança dentro do processo.

**Maturidade:** contrato estavel em evolucao com eventos, fila, workers,
budgets e diagnostico limitados; a supervisao do processo permanece externa.
