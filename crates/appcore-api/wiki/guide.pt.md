# appcore-api

As observações do sync log no próximo major são falíveis. Status privado e
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
resources REST de produto ou schemas de negócio. O host novo normalmente
acessa pelo `appcore-bin`.

Queries de aplicação são autorizadas pela policy de capability composta antes
do router. Queries de status do Runtime permanecem fora do catálogo da
aplicação.

Hosts do Runtime congelam o registro de queries do `ApiRouter` após o bootstrap.
Clones do router compartilham endpoints por `Arc`, então facade direta, HTTP e
peer RPC liberam o mutex do estado do host antes de executar o endpoint.
Queries independentes rodam em paralelo; um `register_query` tardio falha com
`router_frozen`.

O limite configurado aplica-se ao corpo HTTP completo antes de o Axum
desserializar o JSON. Rotas protegidas aceitam exatamente um header
`Authorization` bearer bem formado; duplicatas falham de forma fechada.

`HttpCommandAuth::default()` exige autenticação e falha fechado até que um
verificador de token seja configurado. Apenas
`insecure_local_for_testing()` desativa explicitamente a autenticação de
command/query para testes locais controlados. `/v1/health` permanece público
por contrato. Rejeições de autorização de command geram audit com metadados
normalizados, sem credenciais, payload ou chave de idempotência.

**Maturidade:** superfície HTTP V1 RC estrita e estável.
