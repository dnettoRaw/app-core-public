# appcore-gateway

**Responsabilidade:** relay WebSocket isolado por tenant para conexoes Gateway
entre clients externos e workers AppCore.

**Dependencias internas:** contracts, types, security, distributed
contracts e peer RPC.

**API principal:** `GatewayConfig`, `GatewayState`, estado por tenant, registry
e resolver de capability, conexoes bounded de worker/client,
`MeshPeerTransport`, DTOs de request/response do mesh relay, pruner de
heartbeat e factory do router Axum. Contratos de content-envelope opaco são
reexportados para roteamento de payload cifrado.

> **Blocker de compatibilidade do RC atual:** o acesso direto a
> `GatewayState::tenants` foi removido para que tenants independentes não
> compartilhem um único lock. Embedders devem usar `tenant_partition`,
> `tenant_partition_or_insert`, `tenant_count` e `connection_count`. Os mapas
> V1 de requests pendentes continuam públicos, enquanto o `EnvelopeRouter`
> controla o lifecycle vinculado à generation; observe-os com
> `pending_request_count`. O conflito do diretório de tenants continua sendo
> blocker de release, não autorização para iniciar 2.0. Consulte
> [o guia de migração](../../release/gateway-tenant-migration.md).

## Composicao no Runtime

`appcore-bin` e o composition root. Um deployment habilita esta crate pelo
mapa de adapters ja existente:

```toml
[adapters.gateway]
provider_id = "appcore-gateway"
settings = { bind_address = "127.0.0.1:8080", domain_suffix = "gateway.example.com", heartbeat_interval_ms = "30000", heartbeat_timeout_ms = "90000" }
secret_refs = {}
```

Deployments cluster tambem devem apontar todas as instancias Gateway para o
mesmo arquivo de replay por caminho absoluto em volume compartilhado e gravavel:

```toml
paths = { gateway_replay = "/shared/appcore/gateway-connection-jti.json" }
```

O adapter aceita apenas essas quatro settings. Endpoints, referencias de
segredo, settings desconhecidas e tentativas de configurar autenticacao sao
rejeitados. A autenticacao permanece obrigatoria nas instancias compostas pelo
manifest.

No bootstrap, o host inclui a capability do owner `runtime.gateway` no
catalogo, autoriza-a por `RuntimeCapabilityPolicy`, reutiliza o provider de
seguranca do Runtime e registra o Gateway como servico critico do Supervisor.
Configuracao invalida ou falha de bind aborta o startup; sem
`adapters.gateway`, nenhuma task ou porta e criada.

O gateway resolve o tenant pelo sufixo de dominio definido pelo deployment ou
por parametro de query usado em teste local, autentica conexoes quando
configurado, roteia envelopes Peer RPC e requests HTTP Peer RPC via mesh relay
somente dentro da particao do tenant e remove workers stale mantendo filas de
saida limitadas.

Upgrades autenticados aceitam credencial apenas no header `Authorization`;
credenciais em query sao rejeitadas. Tokens de worker usam
`worker_connection_hash` para vincular tenant, cluster, installation, Core e
capabilities. Tokens de client usam `client_connection_hash` para vincular
tenant, cluster e device. Ambos sao tokens `peer` de uso unico, com `jti`, hash
do request e vida maxima de 60 segundos; o socket expira junto com o token.

O mesh relay valida schema V1, metadata de roteamento do Peer RPC interno,
digest do body e hash assinado antes de encaminhar. O payload da aplicacao
permanece opaco. Frames e mensagens aceitam no maximo 4 MiB; limites de tenant,
conexao, capability, request pendente, timeout, fila e roteamento concorrente
falham fechados. Heartbeat exige o JSON exato, e resposta de worker so e aceita
da geracao de conexao selecionada.

`mesh-relay` e um peer transport para Cores que mantem conexoes Gateway somente
de saida em vez de expor portas locais ou IPs estaveis. Ele nao e sistema de
consenso, terminador TLS publico ou gerenciador de segredos de producao.
Federacao de edge relays e transports alternativos nao podem enfraquecer
autenticacao, expiry, nonce ou replay protection do Peer RPC.

O RC atual inclui o contrato `GatewayRegistryProvider` e a implementacao
`RedisGatewayRegistryProvider`. Ela exige endpoint TLS fora de loopback,
credential resolvida separadamente, limites de timeout/concurrency e scripts
atomicos no hash slot do tenant. Mutacao com resultado ambiguo nunca e repetida;
o caller entra em isolamento e usa `reconnect` explicitamente.
`GatewayHaLifecycle` já fecha admission HTTP/WebSocket, dispatch e completion
fora de `Healthy`, sem alterar single-instance sem HA. `GatewayHaCoordinator`
agora adquire e renova o conjunto completo e limitado de leases por tenant
antes de entrar em `Healthy`; round parcial, stale ou incerto limpa os leases
locais e entra em `Isolated`. Cada round e serializado, limitado a 64 operacoes
concorrentes e cinco segundos totais. `GatewayRuntime::with_ha_coordinator`
possui a task de recovery/shutdown, refaz o snapshot completo e limitado de
workers/sessions antes de `Healthy`, registra sockets novos antes da admission
local e remove records exatos em disconnect ou prune de heartbeat. O caminho
local agora faz claim de epochs origin/target e geracao do worker antes do
dispatch, complete antes de devolver sucesso e cancel em falha de fila, timeout
ou shutdown; future abortado expira em ate 30 segundos. O provider pode conferir
o claim live exato sem consumi-lo antes da admission no target. A rota V2 de federacao
agora tem schema estrito, credential separada de uso unico vinculada ao body e
erros AC-021 tipados. A rota HTTP limitada passa um E2E com dois estados Gateway
e completa o fence antes de aceitar a resposta; a mesma prova passa com Redis
7.4 e via Caddy 2.11.4 sem bypass direto do origin. A certificacao de recovery
apos perda do owner tambem roteia novamente com epoch maior depois do TTL
limitado. AC-022 e evidencias de plataforma ainda sao obrigatorios antes do
deployment; fallback local continua proibido.

O host persiste identidades de conexao de uso unico com o
`FilePeerNonceStore`, seguro entre processos. Standalone usa o storage privado
do Runtime; cluster exige `paths.gateway_replay` absoluto em um volume compartilhado e
gravavel e falha fechado quando ele nao existe ou esta indisponivel. Sockets
ativos expiram com a credencial em no maximo 60 segundos. Embedders usam store
local limitado por default ou injetam `PeerNonceStore` duravel/compartilhado por
`GatewayState::with_replay_store` ou `GatewayRuntime::with_replay_store`.
Rate limit por IP e terminacao TLS continuam no deployment.

`GatewayRuntime` possui listener, thread de runtime, router e pruner. `stop`
primeiro pede shutdown graceful e depois descarta o future do servidor antes
do prazo, fechando conexoes incompletas e fazendo join da thread. `Orphaned`
fica apenas como quarentena defensiva para falha inesperada da thread, nao como
caminho normal de timeout. O snapshot nunca expoe credenciais ou tokens.
Embedders que chamam `spawn_heartbeat_pruner` devem aguardar seu join handle.

Hashes de conexão de worker e client usam framing binário canônico V2 e levam
o marcador `v2:`. Hashes anteriores sem versão não são intercambiáveis;
emissores de token e consumidores Gateway devem ser atualizados juntos.

Cada tenant mantém índices diretos e limitados de worker por Core ID e por
`(cluster_id, core_id)`. Lookups de roteamento são O(1); register, reconnect,
disconnect e prune de heartbeat atualizam mapa, registry de capabilities e
índices sob o mesmo lock do tenant. `worker_index_rebuilds` e
`worker_index_inconsistencies` expõem contadores limitados de saúde do índice.

## Seleção determinística de workers no `1.0.3-rc`

O enum V1 exaustivo `SelectionPolicy` continua limitado a `FirstAvailable`.
`WorkerSelectionPolicy` fornece escolhas opt-in `RoundRobin`, `LeastInflight`,
`HealthWeighted` e `Affinity`, mantendo `FirstAvailable` como default. Quem
consumiu as variantes avançadas no RC deve atualizar o nome do enum; nenhum
manifesto ou contrato wire muda. A ordem
de identidade dos candidatos é estável e não depende da iteração de um
`HashSet`. `CapabilityResolver::select` recebe inputs live limitados e rejeita
capability ausente, worker stale/desconectado, worker esgotado e affinity
inválida com valores distintos de `WorkerSelectionError`.

Affinity não mantém mapa: rendezvous hashing inclui tenant, capability, chave
limitada e identidade do worker. O dispatch Peer RPC e mesh não reescreve o
alvo V1 assinado. Ele impõe independentemente no máximo 64 rotas inflight por
worker, com permit liberado em todo caminho terminal. Assim o planejamento não
contorna admission, e a telemetria expõe outcomes fixos de unhealthy/capacity e
pico inflight por worker sem labels de identidade. Consulte
[`release/gateway-worker-selection-rc.md`](../../release/gateway-worker-selection-rc.md).

## Telemetria limitada no `1.0.4-rc`

`GatewayMetrics::telemetry_snapshot` e `GatewayRuntime::details`
expõem p50/p95/p99 em buckets fixos para rota, espera do worker, lock do tenant
e tamanho de payload. Também expõem inflight/pico, pico de profundidade da fila,
reconnect, retry, autenticação, saturação, timeout, rejeição unhealthy/capacity,
pico inflight por worker, overflow e falha de exporter. No máximo 128 nomes
validados de capability são mantidos; nomes posteriores usam uma única série
fixa de overflow. Tenant, installation, Core, request, connection, credencial,
payload e texto de erro nunca são labels.

`GatewayTelemetryExporter` recebe somente um snapshot próprio quando o operador
chama `export_telemetry`; o roteamento nunca chama exporters nem SDKs de vendor.
Adapters Prometheus/OpenTelemetry pertencem ao deployment e devem limitar suas
filas. Os contadores estáveis 1.0 não mudam; o contrato detalhado é adição do RC.

**Maturidade:** perfil RC de peer transport para a superficie distribuida V1.
