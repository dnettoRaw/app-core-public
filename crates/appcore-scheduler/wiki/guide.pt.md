# appcore-scheduler

`shutdown_with_timeout(duration)` fecha a admissão e solicita cancelamento
cooperativo. `Ok(true)` indica coordenador e workers encerrados; `Ok(false)`
indica que continuam vivos e não devem ser substituídos. A chamada pode ser
repetida para observar a conclusão. `shutdown()` usa orçamento de espera de
cinco segundos e retorna `SchedulerError::Shutdown` se incompleto. `Drop`
solicita shutdown sem esperar e desassocia threads ainda vivas; memória e
efeitos externos podem permanecer até callbacks/providers retornarem. Isso
não mata threads nem garante tempo real estrito. O deployment deve colocar
um scheduler incompleto em quarentena e isolar callbacks não confiáveis em processo.

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** execução local limitada e placement explicável de Core.

**Dependências internas:** `appcore-contracts`, `appcore-core`.

**API principal:** `Scheduler`, `SchedulerConfig`, `ScheduledTask`,
`TaskSchedule`, callback/context/result, retry policy, handle e snapshots;
`DurableSchedulerConfigV1`, `SchedulerStateProvider`, providers em memória e
arquivo, claims e receipts V1;
requests/candidates/rejections/evaluations/decisions de recursos e
`PlacementEngine`.

Use para trabalho local declarado com limites, cancelamento e shutdown. Não é
workflow engine durável nem fila distribuída.

O shutdown fecha a admissão mantendo o lock do estado, e a aritmética de
deadlines é verificada. Tempos one-shot, interval ou retry não representáveis
retornam `InvalidSchedule` ou removem a task esgotada em vez de causar panic.

O scheduler cria um único pool fixo, limitado por `max_concurrent_tasks`, e
uma fila limitada a duas vezes esse número efetivo de workers ou `max_tasks`.
Quando os slots de despacho e a fila estão ocupados, tarefas devidas posteriores
permanecem no registro sem consumir tentativa. Observe a pressão com
`worker_thread_count`, `queued_task_count` e `queue_saturation_count`. O
shutdown fecha a admissão e drena callbacks já aceitos com
`TaskContext::is_cancelled()` marcado. Callbacks não são terminados à força nem
recebem timeout preemptivo porque threads Rust não podem ser interrompidas com
segurança.

`SchedulerConfig` rejeita valores acima de `MAX_SCHEDULER_WORKERS` (64) ou
`MAX_SCHEDULER_TASKS` (65.536). Cada coordinator e worker de callback possui
stack explícita de 1 MiB, impedindo reserva ilimitada de threads.

A varredura de tasks devidas usa um max-heap limitado aos slots de despacho
disponíveis. Ela preserva prioridade decrescente, deadline mais cedo e ordem de
registro, e clona somente os IDs selecionados. Como a fila contém no máximo
duas vezes o número efetivo de workers, o teto global é de 128 records
candidatos mesmo quando todas as 65.536 tasks registradas estão devidas.

O contrato opt-in de estado do `1.0.2-rc` retém somente identidade da task,
hash da definição, next run, attempts, policy de misfire, claim atual, epoch de
fencing e último receipt. Um receipt one-shot confirmado impede execução após
restart. Claim expirado sem receipt tem recovery at-least-once: efeitos do
callback devem usar o epoch exposto ou sua própria fronteira de idempotency. O
provider de referência local ao processo prova claims limitados entre dois
owners. Configure `Scheduler::with_state_provider` e registre apenas trabalho
selecionado com `schedule_durable`; chamadas normais a `schedule` continuam
efêmeras. O provider em arquivo persiste o contrato com locks no processo e
entre processos, snapshot V1 limitado e checksummed e troca atômica. Callbacks
devem aplicar `TaskContext::fencing_epoch` na fronteira do efeito protegido
quando houver owners concorrentes. Veja a
[decisão V1](../../../release/scheduler-state-provider-v1.md).

O I/O do estado em arquivo é limitado antes da alocação. O load decodifica por
um reader com teto de 4 MiB; o save empresta os records ordenados, calcula o
checksum transmitindo o array JSON exato e grava o snapshot completo por um
buffer fixo de 64 KiB. A troca atômica e o checksum V1 não mudaram. O benchmark
do crate valida um snapshot com o máximo de 1.024 records e informa as fases de
memória idle/workload/retained.

A validação de load empresta campos de task, definition, owner e claim, enquanto
a ordenação é comparada ao último record convertido. Um snapshot máximo sem
claims evita 3.072 alocações temporárias de strings sem mudar a validação ou os
bytes V1.

**Maturidade:** perfil RC atual; estado durável é opt-in.
