# appcore-scheduler

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

Callbacks executam em um pool fixo. O pool nunca excede
`max_concurrent_tasks`, e a fila interna é limitada ao menor valor entre duas
vezes o número de workers e `max_tasks`. Trabalho devido excedente permanece
agendado sem consumir retry; `queued_task_count` e `queue_saturation_count`
tornam a pressão observável. O shutdown drena callbacks aceitos com o
cancelamento marcado em `TaskContext`; não existe timeout preemptivo inseguro,
portanto callbacks longos devem cooperar por `is_cancelled`.

O candidato alpha 1.5 fornece recovery opt-in com `SchedulerStateProvider` V1.
Inicie com `Scheduler::with_state_provider` e use `schedule_durable` apenas nas
tasks selecionadas. O Runtime persiste next run, attempts e receipts, renova
claims limitados e expõe o epoch monotônico de fencing ao callback. `FireOnce`
e `Skip` são policies de misfire explícitas. `Scheduler::new` e `schedule`
continuam locais ao processo e offline. O provider em arquivo usa snapshot V1
limitado e checksummed, locks no processo e entre processos, troca atômica e
sync do diretório. O recovery é at-least-once até o commit do receipt.

**Maturidade:** perfil local RC estável; scheduling é local ao processo.
