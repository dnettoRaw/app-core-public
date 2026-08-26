# appcore-sync

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

`HttpSyncTransport` possui um cliente HTTP reutilizável e limitado.
`with_timeout_ms` preserva o deadline V1 uniforme; `with_timeouts` define
deadlines independentes de conexão/admissão, leitura e escrita.

Use para replicação compatível, ordenada e hash-chained. Não ignore identidade
ou protocolo nem trate como RAFT, multi-master ou resolvedor de conflito de
negócio.

O log file é limitado a 256 MiB e a outbox a 64 MiB. IDs de peer e hashes de
checkpoint são validados na escrita e na leitura. O receiver valida o batch
completo, a aritmética de sequence e cada limite de record antes de alterar log
ou checkpoint; um evento inválido no fim não deixa append parcial.

No próximo major, `FileSyncOutbox` usa o journal binário append-only explícito
`appcore-sync-outbox-v2`. Enqueue e ACK sincronizam um único frame encadeado por
hash; leitores varrem apenas o novo tail, e a compactação limitada preserva
atomicamente as mensagens pendentes. Somente um frame final incompleto é
recuperável. Arquivo V1, sem versão, futuro ou com corrupção completa falha
fechado. Drene V1 antes do upgrade e V2 antes do rollback; veja
[`release/outbox-v2-migration.md`](../../release/outbox-v2-migration.md).

No próximo major, `ReplicationLog::len`, `last_index` e `is_empty` retornam
`SyncResult`. Providers persistentes expõem falhas de observação em vez de
substituir por zero ou estado antigo. Consumers precisam tratar o resultado
antes de atualizar; veja
[`release/fallible-replication-log-observations.md`](../../release/fallible-replication-log-observations.md).

**Maturidade:** perfil RC conservador estável com decode V1 estrito.
