# appcore-sync

Release candidate **`appcore-sync 1.0.2-rc.1`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.


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

[Migração de envelopes de aplicação](../../appcore-peer-rpc/wiki/transport-migration.pt.md).

## Causalidade de revisões opacas

Esta secção descreve alterações ainda não publicadas do código-fonte. Verifique o pacote exato do registry antes da adoção pelos consumidores.

`ObjectRevisionV1`, `RevisionChainV1` e `decide_object_revision_v1` classificam `already_applied`, `ancestor_echo`, `applicable`, `concurrent_branch` e `snapshot_required`. A cadeia aceita até 1 024 revisões com identidades de 128 bytes. Autentique cada evento e vincule o tenant antes de construir evidência: a validação da cadeia é estrutural, não criptográfica. Links ausentes, ciclos, objeto incompatível e identidades de evento/revisão reutilizadas são recusados. Igualdade de conteúdo ou datas não prova ancestralidade. Uma reversão intencional usa revisão/evento novos. Avance o checkpoint de transporte separadamente ao aceitar um eco; chame `validate_checkpoint_head_v1` antes de alterar metadados.

## Retransmissão e serialização do checkpoint

Correção ainda não publicada: uma repetição autenticada idêntica recebe ACK aceite sem novos registos; reutilizar um `batch_id` retido com conteúdo divergente falha com `divergent batch_id reuse`. O receiver retém no máximo 10.000 pares ID/digest. A janela é local ao processo, não evidência permanente; após reinício ou expulsão aplica-se a política existente de checkpoint para sequências antigas. Autentique e isole a origem antes de chamar o receiver. Receivers distintos devem partilhar o handle do log quando partilham checkpoints.

Leitura do checkpoint, verificação da sobreposição, append e publicação de sequência/hash usam o mesmo lock do log. Lotes concorrentes não publicam um checkpoint antigo menor nem combinam sequência e hash de lotes diferentes. Erros do provider ainda podem deixar o log atualizado com checkpoint antigo; a repetição verifica a sobreposição antes de publicar o checkpoint. O sender remove o recibo da outbox só após confirmação do transporte. A perda do primeiro ACK provoca retransmissão segura e posterior drenagem. Processos externos e handles de log independentes exigem coordenação transacional do provider/deployment.

## Candidato de release — 2026-10-14

`2.0.0-alpha.5`: alterações próprias.

## Histórico canónico por serviço (fonte não publicada)

`sync::canonical::FileCanonicalHistoryV1` preserva metadados originais de `SyncMessage` e bytes dos eventos, com evidência transferível de época/writer em `CanonicalBatchV1`. `CanonicalStreamV1` identifica tenant/cluster/serviço sem depender do Core ou endereço atual. `CanonicalHeadV1` vincula sequência, hash original do lote e um segundo digest do serviço, época, writer e histórico. Wire, receiver, log e snapshot V1 existentes ficam iguais; não existe importação ou conversão automática.

Usar `create` apenas no provisionamento inicial; reinício usa `open_existing`. Cada operação recarrega e valida o estado durável sob try-lock do SO. Escritas sincronizam temporário exclusivo, fazem rename atómico e sincronizam o diretório onde suportado. ACK perdido aceita registo persistido idêntico após reinício; lacunas, IDs/sobreposições divergentes, épocas antigas e troca de writer na mesma época são recusados. Corrupção ou storage ausente nunca cria histórico novo. Após falha de persistência, reabrir e consultar o resultado: o rename pode já ter ocorrido.

`batches(offset, limit)` devolve até 64 registos completos de linhagem; autenticar o registo inteiro antes de `replicate_authenticated`. Replica apenas histórico canónico, não materialização nem transação dos efeitos da aplicação. Evidência histórica de época não é lease atual. O host autentica tenant/serviço/origem e autoridade histórica antes de admitir registos; hashes não são assinaturas.

`with_verified_head(required, action)` compara o head durável completo e mantém o lock durante o callback. O head exigido vem de uma política de durabilidade confiável, nunca do próprio candidato. Réplica vazia/atrasada ou com sequência igual mas hash diferente não executa a ação. Dentro do callback, compor fence autoritativo exato e atual em torno do commit, sempre com ordem de locks histórico-depois-autoridade. A referência co-localizada é `FileControlPlane::with_fenced_service_write`; não reentrar nos providers. `ServiceLeaseAuthority::check` remoto é observação, não commit remoto atómico. A primitiva sozinha não promove um nó nem emite certificado de prontidão.

Descobrir o writer com `ServiceLeaseAuthority::inspect` para o mesmo tenant/cluster/serviço; resolver o Core por discovery do deployment e autenticar o endpoint. Inspect pode devolver grant expirado. O antigo writer reentra como réplica; divergência é recusada para reconciliação explícita por operador/snapshot validado pela aplicação. Não reiniciar sequência nem mudar origem para ocultar divergência. Rollback externo de autoridade e histórico está fora desta referência; custódia certificada do storage continua necessária.

Limites: ficheiro codificado 16 MiB, 1.024 lotes, 16.384 eventos e payload bruto total 4 MiB, cada evento até 1 MiB. Identidades seguem perfil ASCII alfanumérico/`-_.` de 128 bytes. Esta referência pequena recarrega o ficheiro completo; descodificação JSON tem overhead de alocação antes da admissão descodificada, não é provider streaming de logs grandes. Executar I/O em workers bloqueantes limitados. Capacidade esgotada bloqueia escritas, sem expulsar/compactar evidência. Snapshots da aplicação, política de confirmação da réplica e qualificação física do mesh continuam separados.

Cada append bem-sucedido faz commit imediato; devolver erro no callback não reverte um append anterior. Reabrir e consultar o head durável antes de repetir resultado incerto.

## Durabilidade das cópias canónicas (fonte não publicada)

`DurabilityTargetV1` fixa batch ID original, head completo, Core writer e época. `DurabilityPolicyV1` seleciona explicitamente `local`, `master` ou `master_and_replica`. `evaluate_durability_v1` devolve `DurabilityStatusV1` com `confirmed`, `persisted_core_ids` e `missing_core_ids`; pendente nunca reduz a política automaticamente. Master é o writer original deste alvo imutável, não o Core que discovery devolve agora. Retry mantém alvo e política; pendente/timeout não reverte escrita nem autoriza reexecutar handler da aplicação.

| Política | Cópias canónicas exigidas | Exposição de perda restante |
| --- | --- | --- |
| `Local { core_id }` | Core local escolhido | Perda dessa cópia; não implica confirmação master/réplica |
| `Master` | Core writer original | Perda do storage do writer antes de outra cópia confirmada |
| `MasterAndReplica { replica_core_id }` | Writer original e uma réplica distinta escolhida | Perda de ambas ou domínio de falha partilhado; IDs distintos não provam discos independentes |

`FileCanonicalHistoryV1::durability_target` recupera um limite exato de lote retido. `durability_receipt(target, local_core_id)` recarrega/valida bytes sob lock, verifica todos os campos do alvo, sincroniza ficheiro/diretório onde suportado e emite recibo V1 determinístico. Vincular `local_core_id` à identidade do storage do deployment, nunca a campo do pedido. Um alvo antigo só continua confirmável como ancestral exato retido; permite recuperar resposta perdida após reinício/novos lotes sem novo append. Não exige journal separado de recibos nem muda formato do histórico. Genesis, meio de lote, storage ausente/corrupto/ocupado e alvo divergente não emitem recibo.

Autenticar o recibo completo e vincular `persisted_by_core_id` ao remetente autenticado antes de avaliar. JSON e digests não autenticam. `decode_durability_receipt_v1` limita input a 16 KiB e rejeita versões desconhecidas/ausentes com `NO MORE SUPPORTED PLEASE UPDATE`; avaliação admite até 16 recibos. Evidência repetida de um Core é idempotente e nunca conta duas vezes. Cada recibo deve corresponder ao alvo completo; Core não selecionado não satisfaz a política. Recibo não é lease atual, probe de disponibilidade, prova de materialização da aplicação ou certificado de consenso. Sync depende das garantias certificadas da plataforma/filesystem.

O deployment persiste política escolhida, alvo confiável e confirmações autenticadas na sua fronteira de recuperação antes de reportar sucesso ou retirar o lote da outbox. Deve serializar mutações da fila e vincular retirada ao mesmo conteúdo, evitando corrida baseada só no ID. As primitivas não retêm mapa ilimitado de operações pendentes nem alteram outbox automaticamente. `FollowerSyncClient` mantém o contrato de ACK de transporte; durabilidade na réplica exige composição explícita deste percurso. O teste de integração conserva outbox real em ficheiro enquanto pendente, reinicia após resposta da réplica perdida e só retira após ambos os recibos correspondentes.

As políticas confirmam **cópias do histórico canónico**, não uma transação/outbox local da aplicação anterior à atribuição canónica. Durabilidade offline local-first existente continua na aplicação. Não chamar confirmação master/réplica a uma entrada local de outbox. Um alvo confirmado confiável pode fornecer o head exigido à promoção apenas se o deployment o retiver e autenticar as evidências; lease exata atual e fence atómico continuam obrigatórios. Sem declaração de integração em produção, qualificação de corte de energia ou perda zero.
