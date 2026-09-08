# appcore-sync

Testes locais:

```bash
cargo test -p appcore-sync
```

**Responsabilidade:** replicação conservadora leader-to-follower e helpers de
durabilidade local.

**Dependências internas:** `appcore-core`, `appcore-distributed-contracts`,
`appcore-ops`, `appcore-transport`.

**API principal:** node role/status/peer/heartbeat e `SyncMessage`; codec wire
V1; replication logs/snapshots; checkpoints e outbox memória/arquivo; receiver
state/ack; follower client; HTTP transport; peer discovery; retry, métricas e
`SyncError`.
Contratos de content-envelope opaco são reexportados para pacotes sync
baseados em DNT sem expor plaintext ao código de roteamento. O teto de retenção
`MAX_OPAQUE_MESSAGE_ID_BYTES` também é reexportado e vale 1.024 bytes.

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

`FileSyncCheckpointStore` percorre V1 por um reader fixo de 16 KiB. A validação
de startup não retém mapa de peers, e o lookup valida o arquivo completo
possuindo somente o hash pedido. A mutação ainda monta uma vez o mapa canônico
ordenado, mas o grava diretamente por um buffer fixo sem manter outra `String`
do tamanho do arquivo. Os tetos públicos são 8 MiB, 65.536 records não vazios e
256 bytes UTF-8 por ID de peer; cada linha é limitada antes de ampliar o
scratch. Peers duplicados continuam escolhendo o último valor e a próxima
mutação bem-sucedida os canoniza.

`FileReplicationLog` percorre V1 incrementalmente e mantém somente um vetor
ordenado compacto de pares sequence/índice do record, além de offsets,
tamanhos e digests por record, não todos os payloads. A implementação em
memória usa o mesmo índice plano ordenado e busca binária, evitando o overhead
de buckets de hash nos logs locais limitados. Instâncias concorrentes validam
a âncora da hash chain e leem apenas o novo tail; uma
substituição atômica por snapshot é tratada como nova geração e reconstruída.
`events_page` limita a leitura antes da alocação a 1.024 registros e 48 MiB. O
`events_since` legado permanece compatível em fonte, mas rejeita uma leitura de
arquivo acima desses tetos. Ferramentas de sync do deployment devem usar páginas;
não há CLI de sync do Runtime distribuída. O log aceita no máximo
256 MiB, payloads de 1 MiB e índice com 262.144 registros. Batches HTTP do
Runtime param em 1 MiB de eventos brutos; o envelope JSON V1 codificado aceita
até 5 MiB para transportar até a representação numérica de pior caso de um
payload válido.
O encoder V1 serializa identidade, mensagem e eventos emprestados diretamente
na `String` de saída exigida. Ele não clona o batch completo antes do encode, e
seu JSON continua byte a byte idêntico ao contrato V1 owned.

`ReplicationSnapshot::try_from_records` consome pares sequence/payload owned e
move suas alocações para um snapshot V1 protegido por checksum.
`ReplicationSnapshot::validate` verifica por referência as mesmas invariantes
de formato, quantidade, payload, sequence e checksum, sem criar uma segunda
coleção de payloads. Assim um provider persistente valida antes da mutação com
um único owner do snapshot semântico. Consumidores em memória que possuem o
snapshot podem usar `InMemoryReplicationLog::restore_snapshot_owned` para
validar e mover os payloads diretamente ao log, evitando cópias simultâneas.

No `1.0.2-rc`, `FileSyncOutbox` usa o journal binário append-only explícito
`appcore-sync-outbox-v2`. Enqueue e ACK sincronizam um único frame encadeado por
hash; leitores varrem apenas o novo tail, e a compactação limitada preserva
atomicamente as mensagens pendentes. Somente um frame final incompleto é
recuperável. Arquivo V1, sem versão, futuro ou com corrupção completa falha
fechado. Drene V1 antes do upgrade e V2 antes do rollback; veja
[`release/outbox-v2-migration.md`](../../release/outbox-v2-migration.md).

O estado residente da outbox de arquivo contém somente IDs de batch, offsets do
journal, tamanhos codificados, digests de payload e metadados de retry. O
enqueue mede o JSON em uma passagem limitada e depois o grava por um buffer
fixo de 64 KiB; nenhuma cópia codificada fica residente. Leituras da frente e
de páginas decodificam e verificam uma mensagem indexada por vez. Cada ID é
compartilhado com o estado transacional do scan de tail; refresh clona somente
handles em vez de copiar todos os identificadores pendentes. O snapshot
`messages()` compatível com o código existente ainda retorna um `Vec` owned;
consumidores sensíveis à memória devem usar páginas limitadas.

`InMemorySyncOutbox` também obtém o tamanho codificado exato com um contador
JSON protegido contra overflow, sem alocar e descartar uma mensagem codificada
completa. Um batch válido de 4 MiB mediu 10,92 ms p50 e 17,52 MiB de RSS pico no
Apple M1, contra 23,55 ms e 45,73 MiB com o buffer temporário.

A janela de 10.000 batches processados do receiver retém uma alocação
compartilhada por `batch_id` entre lookup de duplicata e eviction pela ordem de
aceitação. Aplicar 10.000 batches com IDs de 128 bytes mediu 58,27 ms p50 e
15,27 MiB de RSS pico no Apple M1, contra 61,29 ms e 17,45 MiB com strings
duplicadas. As fronteiras do receiver e da outbox rejeitam IDs vazios,
caracteres de controle e IDs acima de 1.024 bytes UTF-8 antes de retê-los. Isso
limita a janela fixa tanto por bytes quanto por quantidade. Rejeição de
duplicata e eviction do mais antigo não mudam.

O contrato aditivo de paginação `SyncOutbox` do `1.0.2-rc` expõe `peek`,
`stats`, `mark_attempt`, `next_ready` e receipts parciais ordenados. Leituras de
página são limitadas a 1.024 mensagens e 48 MiB antes de clonar payloads. O
providers em memória e arquivo implementam paginação e observações de retry
exatas. Attempts e receipts ordenados do arquivo são frames encadeados por hash
que sobrevivem ao restart. Um provider pode usar
`encoded_sync_message_bytes` para obter o tamanho JSON compacto exato sem uma
cópia codificada e `write_sync_message_json` para transmitir essa mesma
representação canônica a um writer limitado. Os escapes continuam compatíveis
com Serde e os bytes dos eventos usam um scratch buffer fixo de 16 KiB. O
receipt é medido e serializado diretamente pelo
writer fixo de 64 KiB; o fixture máximo com IDs escapados não materializa mais
seu buffer JSON de 2.086.913 bytes em produção. O scan empresta IDs sem escape
do próprio frame. Um provider externo que usa os defaults de compatibilidade
continua compilando: retorna no máximo a mensagem da frente,
informa estatísticas estendidas como desconhecidas e rejeita explicitamente
attempts persistidos ou receipts com múltiplas mensagens.

`FollowerSyncClient` usa diretamente esse contrato
limitado. Cada falha de transporte registra a readiness do retry, o sucesso
aplica um receipt exato e a drenagem expõe o último batch confirmado para o
avanço do checkpoint. O snapshot completo `pending_messages` permanece por
compatibilidade de fonte; consumidores novos devem usar `pending_page` e
`outbox_stats`.

No `1.0.2-rc`, `ReplicationLog::len`, `last_index` e `is_empty` retornam
`SyncResult`. Providers persistentes expõem falhas de observação em vez de
substituir por zero ou estado antigo. Consumers precisam tratar o resultado
antes de atualizar; veja
[`release/fallible-replication-log-observations.md`](../../release/fallible-replication-log-observations.md).

**Maturidade:** perfil RC conservador estável com decode V1 estrito.

O default de `ReplicationLog::events_page` é um adapter de leitura completa
para providers externos: valida limites, chama `events_since` e move os payloads
selecionados para o resultado limitado. Ele não limita essa leitura inicial.
Providers precisam sobrescrever a paginação para impor quantidade/bytes antes
de leituras ou clones; os internos de memória/arquivo já fazem isso. Retornar
poucos registros não prova materialização limitada. O teste de consumidor é
`cargo test -p appcore-sync --test external_log_paging`.

## Documentação estável

ID estável: **ACR-012**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-012). Esse ID permanente
continua válido se a página da wiki mudar.
