# appcore-sync

O contrato de observação do `1.0.2-rc` é falível: `ReplicationLog::len`,
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
baseados em DNT sem expor plaintext ao código de roteamento. Seu teto público
de retenção `MAX_OPAQUE_MESSAGE_ID_BYTES` é de 1.024 bytes UTF-8.

`HttpSyncTransport` possui um cliente HTTP reutilizável e limitado. Use
`with_timeout_ms` para o deadline V1 uniforme ou `with_timeouts` para deadlines
independentes de conexão/admissão, leitura e escrita.

Use para replicação compatível, ordenada e hash-chained. Não ignore identidade
ou protocolo nem trate como RAFT, multi-master ou resolvedor de conflito de
negócio.

Para conflitos prontos para UI, use `SyncConflict` e
`InMemorySyncConflictStore`. Os registros contêm somente metadata limitada de
peer/sequence, SHA-256 e motivo tipado. Requests de resolução são idempotentes
e outra decisão não sobrescreve a existente; merge de payload de domínio fica
fora deste crate.

O log file é limitado a 256 MiB e a outbox a 64 MiB. IDs de peer e hashes de
checkpoint são validados na escrita e na leitura. O receiver valida o batch
completo, a aritmética de sequence e cada limite de record antes de alterar log
ou checkpoint; um evento inválido no fim não deixa append parcial.

O arquivo checkpoint V1 aceita no máximo 8 MiB e 65.536 records não vazios,
com IDs de peer limitados a 256 bytes UTF-8. `FileSyncCheckpointStore` valida
uma linha limitada por vez com reader fixo de 16 KiB. O startup não retém mapa
decodificado; um lookup percorre e valida o arquivo completo, mas só aloca se o
alvo existir. Uma mutação monta uma vez o mapa canônico ordenado e o transmite
ao arquivo temporário atômico, portanto nenhuma `String` completa de entrada ou
saída coexiste com esse mapa. Entradas duplicadas preservam o último valor do
comportamento V1 e a próxima mutação grava somente a entrada canônica.

`FileReplicationLog` percorre uma linha V1 limitada por vez e mantém somente um
índice compacto ordenado de sequence para record, além de offsets, tamanhos e
digests. O log em memória usa o mesmo índice plano ordenado e busca binária,
sem manter buckets de hash nos logs locais limitados. Payloads são decodificados sob demanda, um por vez. Um append protegido valida a última âncora da hash chain e percorre
somente bytes adicionados por outra instância; substituição atômica por snapshot
invalida a âncora e reconstrói incrementalmente o índice completo. Use
`events_page(index, max_records, max_bytes)` com tetos de 1.024 registros e
48 MiB. O método completo de compatibilidade rejeita leituras maiores no store
de arquivo. Ferramentas do deployment devem usar páginas; não há CLI de sync do
Runtime distribuída. Os tetos são 256 MiB por arquivo, 1 MiB por
payload e 262.144 registros. Páginas HTTP do Runtime levam até 1 MiB de eventos
brutos em um envelope V1 codificado limitado a 5 MiB, incluindo a pior expansão
da matriz JSON de bytes.
O encoder wire empresta identidade, mensagem e cada evento enquanto grava essa
`String` de saída exigida. Assim ele evita um segundo batch completo na memória
e preserva o JSON V1 owned exato e a validação do node de origem.

Crie snapshots portáveis a partir de payloads já owned com
`ReplicationSnapshot::try_from_records`; cada `Vec<u8>` é transferido para o
snapshot. Providers chamam `ReplicationSnapshot::validate` por `&self` para
verificar versão, quantidade, tamanho por record, sequences não-zero únicas e
checksum sem clonar a coleção de payloads. A validação deve terminar antes de
uma transação de restore alterar o estado durável. Um consumidor em memória
que possui o snapshot pode usar `InMemoryReplicationLog::restore_snapshot_owned`
para validar e mover os payloads ao log sem reter as duas coleções.

A outbox file-backed do `1.0.2-rc` é o journal binário append-only V2
explícito. Enqueue e ACK acrescentam e sincronizam um frame ordinal encadeado
por hash; instâncias atuais varrem somente os novos bytes do tail. A compactação
atômica muda a geração e retém records pendentes. O startup trunca somente um
frame final incompleto e falha fechado em corrupção completa, duplicação,
reordenação ou versão incompatível. V1 nunca é inferido ou convertido: drene V1
antes do upgrade e V2 antes do rollback, seguindo o
[runbook de migração](../../../release/outbox-v2-migration.md).

O índice V2 no processo retém somente ID de batch, ordinal, offset dos dados,
tamanho codificado, digest do payload e metadados de retry. O enqueue primeiro
mede e calcula o hash do JSON, depois serializa a mesma mensagem diretamente
por um buffer fixo de 64 KiB. `front`, `peek` e `next_ready` buscam o record
indexado, decodificam uma mensagem e verificam tamanho e digest exatos. Assim o
journal possui o payload sem virar a fonte da ordem de entrega ou do estado de
retry. O ID de batch é um `Arc<str>` compartilhado pelo índice vivo e pelo scan
transacional do tail. Portanto refresh clona somente handles, não até 1.024
bytes de identificador para cada mensagem pendente; estado enqueue recém-lido
compartilha a mesma alocação com sua operação pendente. Quando a memória for
limitada, use os métodos paginados: `messages()`
precisa materializar todas as mensagens pedidas porque seu retorno público é um
`Vec`.

O provider em memória mede esse mesmo tamanho JSON exato analiticamente com
aritmética protegida contra overflow. `encoded_sync_message_bytes` oferece essa
contagem aos providers de integração, enquanto `write_sync_message_json`
transmite a representação idêntica e compatível com Serde por um scratch buffer
fixo de 16 KiB para eventos. Nenhum caminho cria uma segunda mensagem codificada
completa apenas para decidir admissão, limites de página, persistência ou
`pending_bytes`. Em
um batch válido de 4 MiB no Apple M1, o p50 caiu de 23,55 ms para 10,92 ms e o
RSS pico de 45,73 MiB para 17,52 MiB.

O receiver também guarda uma alocação compartilhada por `batch_id` processado
entre seu set de duplicatas e a fila ordenada de eviction. A janela permanece
fixa em 10.000 IDs. Aplicar 10.000 batches com IDs de 128 bytes no Apple M1
mediu 58,27 ms p50 e reduziu RSS pico de 17,45 MiB para 15,27 MiB, sem mudar a
rejeição de duplicatas nem a eviction do mais antigo. As fronteiras do receiver
e da outbox rejeitam ID vazio, caracteres de controle ou mais de 1.024 bytes
UTF-8 antes de reter a mensagem. `SyncMessage::new` continua sendo um construtor
de dados infalível; a aceitação é decidida nessas fronteiras com estado.

A extensão de outbox do `1.0.2-rc` pagina com `peek(limit, max_bytes)`,
expõe `stats` sem payload, registra readiness de retry com `mark_attempt`,
seleciona somente o prefixo ordenado pronto com `next_ready` e aplica receipts
parciais de prefixo exato. Os tetos globais são 1.024 mensagens e 48 MiB. Os
defaults de compatibilidade nunca chamam `messages()`: providers anteriores à
extensão expõem uma mensagem imediata da frente, estatísticas estendidas
desconhecidas e erros explícitos para estado que não conseguem persistir.

`FileSyncOutbox` registra cada attempt da mensagem da frente e cada receipt
validado como frame V2 limitado e encadeado por hash. Contadores/readiness de
retry sobrevivem ao restart; attempt ou receipt completo corrompido falha
fechado, enquanto frame final incompleto retém o prefixo não confirmado. O JSON
do receipt é medido primeiro e serializado diretamente pelo writer fixo de 64
KiB. O fixture máximo de 1.024 IDs escapados possui 2.086.913 bytes e não fica
mais retido como um `Vec` adicional em produção. O scan empresta IDs sem escape
do frame existente e aloca strings de identificador somente quando precisa
desfazer escapes.

O follower aciona diretamente `next_ready`, `mark_attempt` e receipts exatos.
Use `pending_page`, `outbox_stats` e `flush_pending_with_progress` para inspeção
limitada e avanço do checkpoint. A entrega do Runtime nunca chama o snapshot
completo de compatibilidade.

O default de `ReplicationLog::events_page` é um adapter de leitura completa
para providers externos: valida limites, chama `events_since` e move os payloads
selecionados para o resultado limitado. Ele não limita essa leitura inicial.
Providers precisam sobrescrever a paginação para impor quantidade/bytes antes
de leituras ou clones; os internos de memória/arquivo já fazem isso. Retornar
poucos registros não prova materialização limitada. O teste de consumidor é
`cargo test -p appcore-sync --test external_log_paging`.

**Maturidade:** perfil RC conservador estável com decode V1 estrito.

Para transferências opacas retomáveis, use `split_sync_payload` e envie os
chunks para `SyncChunkAssembler` em qualquer ordem. Retome pelos intervalos
sem payload de `progress().missing` e chame `assemble` apenas quando todos os
bytes chegarem. O adaptador limita tamanhos, verifica os dois SHA-256, aceita
repetições idênticas e rejeita sobreposições conflitantes sem alterar o V1.
