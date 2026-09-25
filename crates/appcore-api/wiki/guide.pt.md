# appcore-api

A entrada de command/query admite até 16 requests por host antes de coletar
ou decodificar corpos. Clones do router compartilham a barreira; hosts criados
separadamente não. Saturação retorna HTTP 503 sem fila de espera; a recepção
tem prazo de 10 segundos (408), e corpos grandes demais retornam 413. Cada
corpo admitido respeita `max_payload_bytes`: o teto de bytes brutos é 16 vezes
esse valor, não o RSS total. Objetos decodificados, respostas e alocações do
transporte são adicionais. Health/status não passam por essa barreira.
O limite separado de dispatch bloqueante por processo continua ativo.

As observações do `appcore-sync 1.0.2-rc` são falíveis. Status privado e
diagnostics expõem `sync_log_len: null` com
`sync_log_observation_ok: false` quando o provider ao vivo não pode ser lido,
sem relatar estado antigo.

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

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
contratos pelo `appcore-sdk`; a composição HTTP é explícita.

Queries de aplicação são autorizadas pela policy de capability composta antes
do router. Queries de status do Runtime permanecem fora do catálogo da
aplicação.

Hosts do Runtime congelam o registro de queries do `ApiRouter` após o bootstrap.
Clones do router compartilham endpoints por `Arc`, então facade direta, HTTP e
peer RPC liberam o mutex do estado do host antes de executar o endpoint.
Queries independentes rodam em paralelo; um `register_query` tardio falha com
`router_frozen`.
`query_names_iter` empresta o registro congelado para validação interna,
enquanto `query_names` mantém a ordem owned determinística nas fronteiras de
output. Assim, o check do manifest percorre nomes sem clonar o catálogo inteiro.

No `1.0.2-rc`, `ReloadableRuntimeHttpHost` fornece uma transação
explícita de geração de routing. `prepare` aceita apenas geração mais nova no
mesmo endereço já ligado. `reload` executa `/v1/health` antes da ativação, troca
atomicamente o routing de novos requests, verifica a saúde novamente e drena o
in-flight antigo. Se a saúde após a troca ou o drain falhar, a geração anterior
é restaurada e a geração com falha fecha a admissão antes da limpeza. Um
request admitido nunca muda de router. Timeouts são positivos e limitados a 60
segundos; snapshots não contêm identidades de requests. O owner mantém no
máximo uma geração ativa e uma em drain. Uma geração com falha que ainda possui
requests bloqueia o próximo reload, e o último permit libera o Router sem uma
task de limpeza. `generation_snapshot` expõe esse estado limitado sem payload
ou histórico. Cancelar depois da troca restaura sincronicamente a geração
anterior antes de reabrir a admissão.

Mudanças de endereço ficam fora desta primitiva de listener estável. A
composition root deve preparar um segundo listener e coordená-lo pelo
Supervisor existente. Não há watcher automático de manifest V1 nem fallback.
Para validar o bind antes do startup no endereço estável, a composition root
pode transferir um listener TCP já ligado por
`run_on_listener_until_shutdown`.

O limite configurado aplica-se ao corpo HTTP completo antes de o Axum
desserializar o JSON. Rotas protegidas aceitam exatamente um header
`Authorization` bearer bem formado; duplicatas falham de forma fechada.

O host TCP integrado fecha a conexão após 10 segundos sem progresso de
leitura, inclusive com headers HTTP incompletos. Não é possível formar uma
resposta HTTP antes de existir um request, portanto a inatividade nos headers
fecha o socket. Inatividade no corpo após um request completo ainda retorna
HTTP 408.

A validação da query estruturada transmite o JSON para um writer contador
limitado. Assim, aplica o limite exato de bytes serializados sem reter um
`Vec<u8>` codificado, mantendo compatível o método público `payload_bytes()`.
O caminho HTTP valida uma vez antes de cruzar o dispatch blocking.

O router possui um único `RuntimeStaticInfo` imutável compartilhado; clonar o
estado do request não copia listas de peers, seeds DNS, paths ou strings de
identidade. O dispatch blocking recebe ownership dos requests de command/query.
O audit de query mantém somente o ID e o nome limitados enquanto o payload está
em trânsito.
Use `CommandRequest::into_envelope` quando o caller possui o request: ele mantém
a validação V1 e move a alocação UTF-8 existente para o payload binário do core.
O método compatível `to_envelope` atende requests emprestados.

`CommandTokenVerifier` também possui métodos aditivos para requests emprestados.
Os defaults materializam `RequestValidationDetails` e chamam os métodos owned
existentes, portanto verifiers existentes mantêm o comportamento. O verifier do
Runtime os sobrepõe para hashear texto ou JSON estruturado diretamente, sem uma
cópia owned do payload.

Dispatch de command e query compartilha 16 permits blocking no processo. O
runtime usa no máximo 16 threads blocking com stacks de 1 MiB e remove threads
ociosas após cinco segundos. Gate cheio retorna HTTP 503 antes da admissão.

A query interna `runtime.audit` limita `limit` a 1.000. Ela obtém snapshots
compartilhados de registros e entradas sob locks curtos e materializa somente a
página mais nova pedida depois de liberá-los. Nenhuma fila completa de 10.000
itens sofre clone profundo. A seleção compartilhada de 1.000 em 10.000 mediu
2,06 us p50 e 11,88 MiB de RSS pico, contra 4,16 ms e 20,33 MiB para cópias
owned integrais.

`runtime.events` usa a mesma fronteira de snapshot, continua limitando a página
mais nova a 1.000 e omite payloads opacos da resposta inalterada. Selecionar
1.000 de 10.000 eventos mediu 2,39 us p50 e 8,48 MiB de RSS pico, contra
2,09 ms e 14,59 MiB para clonar o histórico completo.

`HttpCommandAuth::default()` exige autenticação e falha fechado até que um
verificador de token seja configurado; `HttpCommandAuth::required` instala um
explicitamente. `insecure_local_for_testing()` só existe nos testes do crate ou
em builds debug com `insecure-testing`, e os hosts embutidos rejeitam essa
policy em listener não-loopback. Reload não pode mudar a fronteira de
autenticação. `/v1/health` permanece público por contrato, mas retorna apenas
`status`; detalhes do Supervisor continuam autenticados. Rejeições de
autorização de command geram audit sem credenciais, payload ou chave de
idempotência. TLS de entrada continua sendo fronteira do deployment.

**Maturidade:** superfície HTTP V1 RC estrita e estável.
