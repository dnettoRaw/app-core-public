# appcore-scheduler

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

O contrato opt-in de estado do candidato alpha 1.5 retém somente identidade da task,
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

**Maturidade:** perfil local RC estável; estado durável é opt-in no candidato alpha 1.5.
