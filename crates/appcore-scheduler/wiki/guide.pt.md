# appcore-scheduler

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** execução local limitada e placement explicável de Core.

**Dependências internas:** `appcore-contracts`, `appcore-core`.

**API principal:** `Scheduler`, `SchedulerConfig`, `ScheduledTask`,
`TaskSchedule`, callback/context/result, retry policy, handle e snapshots;
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

**Maturidade:** perfil local RC estável; scheduling é local ao processo.
