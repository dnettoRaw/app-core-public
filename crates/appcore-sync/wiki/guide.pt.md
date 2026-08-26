# appcore-sync

O contrato de observação do próximo major é falível: `ReplicationLog::len`,
`last_index` e `is_empty` retornam `SyncResult`. Trate erro como health de
persistência desconhecido; nunca substitua por zero ou valor em cache. Migration
e rollback estão em
[`release/fallible-replication-log-observations.md`](../../../release/fallible-replication-log-observations.md).

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** replicação conservadora leader-to-follower e helpers de
durabilidade local.

**Dependências internas:** `appcore-core`, `appcore-distributed-contracts`,
`appcore-ops`, `appcore-transport`.

**API principal:** node role/status/peer/heartbeat e `SyncMessage`; codec wire
V1; replication logs/snapshots; checkpoints e outbox memória/arquivo; receiver
state/ack; follower client; HTTP transport; peer discovery; retry, métricas e
`SyncError`.
Contratos de content-envelope opaco são reexportados para pacotes sync
baseados em DNT sem expor plaintext ao código de roteamento.

`HttpSyncTransport` possui um cliente HTTP reutilizável e limitado. Use
`with_timeout_ms` para o deadline V1 uniforme ou `with_timeouts` para deadlines
independentes de conexão/admissão, leitura e escrita.

Use para replicação compatível, ordenada e hash-chained. Não ignore identidade
ou protocolo nem trate como RAFT, multi-master ou resolvedor de conflito de
negócio.

O log file é limitado a 256 MiB e a outbox a 64 MiB. IDs de peer e hashes de
checkpoint são validados na escrita e na leitura. O receiver valida o batch
completo, a aritmética de sequence e cada limite de record antes de alterar log
ou checkpoint; um evento inválido no fim não deixa append parcial.

A outbox file-backed do próximo major é o journal binário append-only V2
explícito. Enqueue e ACK acrescentam e sincronizam um frame ordinal encadeado
por hash; instâncias atuais varrem somente os novos bytes do tail. A compactação
atômica muda a geração e retém records pendentes. O startup trunca somente um
frame final incompleto e falha fechado em corrupção completa, duplicação,
reordenação ou versão incompatível. V1 nunca é inferido ou convertido: drene V1
antes do upgrade e V2 antes do rollback, seguindo o
[runbook de migração](../../../release/outbox-v2-migration.md).

**Maturidade:** perfil RC conservador estável com decode V1 estrito.
