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

Testes locais:

```bash
cargo test -p appcore-scheduler
```

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

A configuração rejeita mais que `MAX_SCHEDULER_WORKERS` (64) threads de
callback ou `MAX_SCHEDULER_TASKS` (65.536) tasks registradas. Coordinator e
workers usam stacks explícitas de 1 MiB.

Cada varredura de tasks devidas retém somente as melhores candidatas que cabem
nos slots de despacho disponíveis. O max-heap limitado preserva prioridade
decrescente, deadline mais cedo e ordem de registro, clonando IDs apenas das
candidatas retidas. No máximo configurado, 65.536 tasks devidas retêm no
máximo 128 records candidatos em vez de materializar o conjunto completo.

O candidato `1.0.2-rc` fornece recovery opt-in com `SchedulerStateProvider` V1.
Inicie com `Scheduler::with_state_provider` e use `schedule_durable` apenas nas
tasks selecionadas. O Runtime persiste next run, attempts e receipts, renova
claims limitados e expõe o epoch monotônico de fencing ao callback. `FireOnce`
e `Skip` são policies de misfire explícitas. `Scheduler::new` e `schedule`
continuam locais ao processo e offline. O provider em arquivo usa snapshot V1
limitado e checksummed, locks no processo e entre processos, troca atômica e
sync do diretório. O recovery é at-least-once até o commit do receipt.

O provider de arquivo decodifica por um reader limitado a 4 MiB. Save e
checksum emprestam os records recuperados, calculam o hash incrementalmente e
serializam direto por um buffer fixo de 64 KiB no arquivo temporário exclusivo.
Assim os bytes V1 permanecem exatos sem coexistirem um buffer do arquivo, uma
segunda lista de DTOs e outra cópia JSON codificada.

A validação de load também empresta campos de task, definition, owner e claim e
verifica a ordenação contra o último record convertido. Assim, um snapshot
máximo sem claims evita 3.072 alocações temporárias de strings sem alterar as
verificações ou os bytes V1.

**Maturidade:** perfil local RC estável; scheduling é local ao processo.

## Documentação estável

ID estável: **ACR-014**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-014). Esse ID permanente
continua válido se a página da wiki mudar.
