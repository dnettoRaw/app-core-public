# appcore-sync

Release candidate **`appcore-sync 1.0.2-rc.1`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.


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

`SyncConflict` é um contrato UI limitado e sem payload, contendo peer, sequence,
SHA-256 local/remoto e motivo tipado. `InMemorySyncConflictStore` registra
conflitos e aplica `SyncConflictResolutionRequest` de forma idempotente; repetir
a mesma chave devolve um receipt, enquanto outra resolução não sobrescreve a
primeira. O store registra decisões, mas não faz merge de payloads de domínio
nem ignora as regras leader-to-follower.

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

Para payloads opacos maiores que uma requisição, `split_sync_payload` cria
chunks limitados com SHA-256 do chunk e do payload completo.
`SyncChunkAssembler` aceita chunks fora de ordem, informa intervalos ausentes
sem payload e trata repetição idêntica de forma idempotente. Intervalos
inválidos, sobreposições e divergências de digest falham de forma segura; o
wire V1 permanece inalterado.

[Migração de envelopes de aplicação](../appcore-peer-rpc/wiki/transport-migration.pt.md).

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
