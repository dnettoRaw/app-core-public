# appcore-api

A entrada de command/query admite até 16 requests por host antes de coletar
ou decodificar corpos. Clones do router compartilham a barreira; hosts criados
separadamente não. Saturação retorna HTTP 503 sem fila de espera; a recepção
tem prazo de 10 segundos (408), e corpos grandes demais retornam 413. Cada
corpo admitido respeita `max_payload_bytes`: o teto de bytes brutos é 16 vezes
esse valor, não o RSS total. Objetos decodificados, respostas e alocações do
transporte são adicionais. Health/status não passam por essa barreira.
O limite separado de dispatch bloqueante por processo continua ativo.

Testes locais:

```bash
cargo test -p appcore-api
```

**Responsabilidade:** host HTTP de command/query/status e DTOs de transporte.

**Dependências internas:** `appcore-core`, `appcore-security` e
`appcore-supervisor`.

**API principal:** `CommandRequest`/`CommandResponse`,
`QueryRequest`/`QueryResponse`, validation errors, `CommandEndpoint`,
`QueryEndpoint`, `ApiRouter`, `ApiRequest`/`ApiResponse`, `RuntimeHttpHost`,
`HttpApiConfig`, status estático, policy de capability para commands e queries
de aplicação, verificação de token e view do sync log.

Use para rotas do Runtime e queries registradas da aplicação. Não adicione
resources REST de produto ou schemas de negócio. Aplicações novas usam os
contratos de aplicação pelo `appcore-sdk`; a composição HTTP é explícita.

Queries de aplicação são autorizadas pela policy de capability composta antes
do router. Queries de status do Runtime permanecem fora do catálogo da
aplicação.

Dispatch de command e query compartilha 16 slots blocking no processo. O pool
blocking do Tokio usa o mesmo teto, stacks de 1 MiB e remove threads ociosas em
cinco segundos. Saturação retorna HTTP 503 antes de entrar na fila.

Hosts do Runtime congelam o registro de queries do `ApiRouter` após o bootstrap.
Clones do router compartilham endpoints por `Arc`; facade direta, HTTP e peer
RPC liberam o mutex do estado do host antes de chamar o endpoint, permitindo
execução concorrente de queries independentes.
`query_names_iter` expõe o registro congelado por referência para validação;
`query_names` continua sendo a API owned determinística para fronteiras de
output. A composition root usa a visão emprestada, então um manifest válido não
clona nem ordena o catálogo completo de queries.

O `ReloadableRuntimeHttpHost`, opt-in do `1.0.2-rc`, mantém um listener
enquanto valida a saúde e troca gerações de routing de forma atômica. Requests
já admitidos continuam no router antigo até terminar; a geração anterior é
drenada com prazo. Falha no prepare, no health gate posterior à troca ou no
drain mantém ou restaura a geração anterior. Gerações são monotônicas, reloads
são serializados, e o owner retém no máximo uma geração ativa e uma em drain.
Uma geração com falha bloqueia outro reload até seu último request liberá-la.
Snapshots sem payload expõem admissão e in-flight das gerações ativa/em drain
sem reter histórico. Cancelar após a troca restaura a geração anterior de forma
síncrona. Mudança de endereço falha explicitamente e exige uma geração de
listener preparada pela composition root. `RuntimeHttpHost` não muda.

Composition roots que precisam validar o bind antes do startup podem chamar
`run_on_listener_until_shutdown` com um listener TCP já ligado. A posse é
transferida ao host e o shutdown continua gracioso.

Quando composto com `appcore-sync 1.0.2-rc`, `SyncLogView::len` e
`is_empty` são falíveis. O status JSON privado retorna `sync_log_len: null` junto de
`sync_log_observation_ok: false` quando a persistência ao vivo não pode ser
observada; ele nunca substitui um contador estático antigo.

A query interna `runtime.audit` limita `limit` a 1.000. Ela captura snapshots
compartilhados de registros e entradas mantendo apenas locks curtos e depois
materializa a página mais nova solicitada com os locks liberados; nunca clona
profundamente as filas completas de 10.000 itens para uma resposta limitada.

`runtime.events` segue a mesma regra: empresta no máximo os 1.000 eventos mais
novos de um snapshot compartilhado depois de liberar os locks do host e do
event bus. O formato da resposta não muda e continua omitindo payloads opacos.

O limite configurado aplica-se ao corpo HTTP completo antes de o Axum
desserializar o JSON. Rotas protegidas aceitam exatamente um header
`Authorization` bearer bem formado; duplicatas falham de forma fechada.

O host TCP integrado fecha uma conexão após 10 segundos sem progresso de
leitura, inclusive quando o cliente para durante os headers HTTP. Como ainda
não existe request antes de os headers terminarem, esse caso fecha o socket em
vez de devolver um status HTTP. Um request já formado cujo corpo para continua
recebendo HTTP 408 do middleware de ingresso.

`QueryRequest::validate` mede o JSON estruturado por um writer contador
limitado, sem alocar uma cópia codificada completa. O limite exato V1 e o
método compatível `payload_bytes()` não mudam; o HTTP valida uma única vez antes
do dispatch blocking.

O router possui um único `RuntimeStaticInfo` imutável compartilhado; clonar o
estado do request não copia listas de peers, seeds DNS, paths ou strings de
identidade. O dispatch blocking recebe ownership dos requests de command/query.
O audit de query mantém somente o ID e o nome limitados enquanto o payload está
em trânsito.
Os caminhos owned de command usam `CommandRequest::into_envelope`, que valida
os mesmos campos V1 e transfere a alocação do payload para `CommandEnvelope` sem
copiar seus bytes. `to_envelope` permanece para callers emprestados.

`CommandTokenVerifier` também possui métodos aditivos para requests emprestados.
Os defaults materializam `RequestValidationDetails` e chamam os métodos owned
existentes, portanto verifiers existentes mantêm o comportamento. O verifier do
Runtime os sobrepõe para hashear texto ou JSON estruturado diretamente, sem uma
cópia owned do payload.

`HttpCommandAuth::default()` exige autenticação e falha fechado até que um
verificador de token seja configurado. Apenas
`insecure_local_for_testing()` desativa explicitamente a autenticação de
command/query para testes locais controlados. `/v1/health` permanece público
por contrato. Rejeições de autorização de command geram audit com metadados
normalizados, sem credenciais, payload ou chave de idempotência.

**Maturidade:** superfície HTTP V1 RC estrita e estável.

## Documentação estável

ID estável: **ACR-009**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-009). Esse ID permanente
continua válido se a página da wiki mudar.
