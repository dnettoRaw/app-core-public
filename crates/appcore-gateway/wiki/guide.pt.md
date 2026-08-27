# appcore-gateway

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** relay WebSocket isolado por tenant para conexoes Gateway
entre clients externos e workers AppCore.

**Dependencias internas:** contracts, types, security, distributed
contracts e peer RPC.

**API principal:** `GatewayConfig`, `GatewayState`, estado por tenant, registry
e resolver de capability, conexoes bounded de worker/client,
`MeshPeerTransport`, DTOs de request/response do mesh relay, pruner de
heartbeat e factory do router Axum. Contratos de content-envelope opaco são
reexportados para roteamento de payload cifrado.

> **Migração candidata 1.5:** o acesso direto a
> `GatewayState::tenants` foi removido para que tenants independentes não
> compartilhem um único lock. Use `tenant_partition`,
> `tenant_partition_or_insert`, `tenant_count` e `connection_count`. Os mapas
> de requests pendentes agora são privados; use `pending_request_count` para
> observação e deixe o `EnvelopeRouter` controlar seu ciclo. Esta
> mudança não pode ser publicada como 1.0.x; a migração completa está em
> `release/gateway-tenant-migration.md`.

O gateway resolve o tenant pelo sufixo de dominio definido pelo deployment ou
por parametro de query usado em teste local, autentica conexoes quando
configurado, roteia envelopes Peer RPC e requests HTTP Peer RPC via mesh relay
somente dentro da particao do tenant e remove workers stale mantendo filas de
saida limitadas.

O caminho normal de ativacao no Runtime usa o mapa de adapters do Deployment
Manifest:

```toml
[adapters.gateway]
provider_id = "appcore-gateway"
settings = { bind_address = "127.0.0.1:8080", domain_suffix = "gateway.example.com", heartbeat_interval_ms = "30000", heartbeat_timeout_ms = "90000" }
secret_refs = {}
```

Modo cluster tambem exige `paths.gateway_replay` absoluto apontando para arquivo em
volume compartilhado e gravavel por todas as instancias Gateway.

O parser aceita apenas essas quatro settings sem segredo. Endpoints,
referencias de segredo, settings desconhecidas e overrides de autenticacao
falham fechados. `appcore-bin` inclui e autoriza o descriptor do owner
`runtime.gateway` no catalogo compartilhado, reutiliza a seguranca do Runtime e
registra a instancia como servico critico do Supervisor. Sem
`adapters.gateway`, nao existe runtime, listener ou task de Gateway.

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
consenso, terminador TLS publico ou gerenciador de segredos de producao. HA do
gateway, federacao de edge relays e transports alternativos continuam trabalho
futuro e nao podem enfraquecer autenticacao, expiry, nonce ou replay protection
do Peer RPC.

O host usa `FilePeerNonceStore` duravel e seguro entre processos: standalone o
mantem no storage privado, enquanto cluster falha fechado sem
`paths.gateway_replay` absoluto em arquivo compartilhado e gravavel. Sockets expiram em
no maximo 60 segundos. Embedders podem injetar outro `PeerNonceStore`; o default
deles e local e limitado. Rate limit por IP e terminacao TLS ficam no deployment.

`GatewayRuntime` possui listener, runtime Tokio current-thread, router, pruner
de heartbeat e thread. O startup faz bind sincronamente, portanto endereco
invalido ou ocupado aborta o host. O shutdown cooperativo limitado faz join de
todo o trabalho. Antes do prazo ele descarta o future do servidor, fechando
conexoes lentas ou incompletas antes do join da thread. `Orphaned` e apenas
quarentena defensiva de falha da thread. Snapshots seguros contem apenas
lifecycle, enderecos de bind e contadores. Usuarios
diretos de `spawn_heartbeat_pruner` devem guardar e aguardar o join handle.

Hashes de conexão de worker e client usam framing binário canônico V2 e levam
o marcador `v2:`. Hashes anteriores sem versão não são intercambiáveis;
emissores de token e consumidores Gateway devem ser atualizados juntos.

Cada tenant mantém índices diretos e limitados por Core ID e por
`(cluster_id, core_id)`. O lookup de roteamento é O(1). Register, reconnect,
disconnect e prune de heartbeat atualizam mapa primário, registry de
capabilities e índices sob o mesmo lock do tenant. Contadores saturados de
rebuild e inconsistência expõem saúde sem labels ilimitadas.

## Ownership do registry HA (contrato `1.0.2-rc`)

`GatewayRegistryProvider` define leases assíncronos por tenant para instância,
ownership de worker/session, resolução limitada e claim/completion de request
em voo. `GatewayInstanceLease` carrega epoch monotônico;
`GatewayWorkerRecord` também vincula a geração da conexão local; e
`GatewayRequestFence` vincula epoch de origin, epoch de target e geração do
worker. Toda mutação deve comparar esses valores atomicamente.

`GatewayFederationUrl` aceita HTTPS ou HTTP somente em loopback, rejeita
credenciais embutidas e redige o valor no `Debug`. Records de request e session
também omitem suas identidades no debug.

`RedisGatewayRegistryProvider` implementa esse contrato. Configure com
`RedisGatewayRegistryConfig`, converta o `ResolvedSecret` do deployment com
`RedisGatewayCredential::new(secret.into_zeroizing())` e entregue esse owner ao
`connect`; credential não é aceita no endpoint. Redis sem TLS
é limitado a loopback e endpoints remotos exigem `rediss://`. Timeout máximo é
5 segundos, concurrency máxima 64, leases de instância/worker no máximo 60
segundos e resolução no máximo 1.024 workers. Scripts por tenant impõem 1.024
workers, 4.096 sessions e 2.048 requests pendentes.

Incerteza de transporte retorna `Unavailable` sem repetir mutação ambígua. O
owner do lifecycle deve entrar em isolamento e chamar `reconnect` explicitamente
antes de adquirir epoch maior. `GatewayHaLifecycle` expõe os modos fixos
`Stopped`, `Recovering`, `Healthy` e `Isolated`, além de contadores limitados de
transição/recovery/fencing. Anexá-lo com `GatewayState::with_ha_lifecycle` faz
admission HTTP/WebSocket, dispatch de request e completion de response falharem
fechado fora de `Healthy`. Estado sem ele preserva o comportamento
single-instance.

`GatewayHaCoordinator` possui uma lista fixa, unica e limitada de bindings
tenant/cluster para uma instancia. Ele adquire todo epoch antes de `Healthy`,
renova o conjunto exato completo, desfaz aquisicoes concluidas depois de falha
parcial e limpa todos os leases locais em renewal stale ou incerto. Os rounds
sao serializados, usam no maximo 64 operacoes de provider em paralelo e tem
deadline total de cinco segundos. O loop cooperativo tenta recovery novamente
enquanto isolado e libera leases exatos depois de fechar admission.

`GatewayRuntime::with_ha_coordinator` possui esse loop e fornece o snapshot
local. Recovery registra novamente todo worker live limitado e session nao
expirada antes de `Healthy`. Socket novo entra no shared registry antes da
admission local; disconnect, prune de heartbeat e shutdown removem o record
exato. Telemetria do snapshot expoe apenas lifecycle e contagens fixas de
ownership.

O caminho local agora faz claim de epochs origin/target e geracao do worker
antes do dispatch, complete do fence antes de devolver sucesso e cancel depois
de falha de fila, timeout ou shutdown. Um future de rota abortado pelo owner
deixa apenas um record do provider limitado pelo TTL de request de 30 segundos.
O target pode conferir o claim live exato sem consumi-lo antes da admission.
Contadores fixos expoem claims, completions e cancellations sem labels de
request.
O schema estrito de federacao V2 vincula esse fence e a request interna a uma
credential separada de uso unico e retorna erros AC-021 tipados. A rota HTTP
limitada passa um E2E com dois estados Gateway e completa o fence antes de
aceitar a resposta. A prova combinada de deployment usa Redis 7.4 e Caddy
2.11.4 sem bypass direto do origin, perde o owner abruptamente e volta a rotear
via Caddy com epoch maior depois do TTL limitado do lease. Certificacao de
plataforma ainda esta pendente.

O harness local AC-022 tambem mede lookup compartilhado e recovery completo
com 1, 100 e 1.000 tenants, depois 64 rotas com sucesso por cada caminho local
e federado. Ele usa provider em processo para isolar o overhead do contrato; a
evidencia combinada Redis, proxy e perda de owner continua como teste de
deployment ignorado separado.
Ainda e necessaria evidencia de CI de plataforma antes de chamar o profile de
duas instancias de pronto para deployment. O diretorio local nunca vira
fallback de verdade.

## Seleção de workers (`1.0.3-rc`)

`FirstAvailable` permanece o default compatível e agora usa ordem estável de
identidade. As policies opt-in `RoundRobin`, `LeastInflight`, `HealthWeighted`
e `Affinity` operam somente no registry de capability do tenant atual. Use o
selector live antes de construir e assinar o alvo Peer RPC explícito:

```rust
use appcore_gateway::{
    CapabilityResolver, WorkerSelectionInput, WorkerSelectionPolicy,
};
use std::time::Duration;

tenant.resolver = CapabilityResolver::with_policy(WorkerSelectionPolicy::LeastInflight);
let selected = tenant.select_worker(
    &capability,
    WorkerSelectionInput::new(now_ms, Duration::from_secs(90)),
)?;
```

O enum V1 exaustivo `SelectionPolicy` continua limitado a `FirstAvailable`.
Policies avançadas usam o novo `WorkerSelectionPolicy` não exaustivo; isso
preserva a compatibilidade de código-fonte dos consumidores V1 estáveis.

Todas as policies live rejeitam workers fechados/stale, filas de saída cheias
e workers no limite inflight. Health weighting usa pesos fixos de 1 a 16 pela
idade do heartbeat. Affinity aceita no máximo 128 bytes e usa rendezvous
hashing stateless por tenant, sem manter mapa de chaves. O dispatch real
adquire independentemente um permit de 64 rotas por worker e o libera em
sucesso, falha, timeout, cancelamento e shutdown. O Gateway nunca reescreve o
alvo V1 assinado nem faz fallback silencioso de policy.
As medições limpas de referência estão no
[benchmark de seleção de workers do Gateway](benchmarks/gateway-worker-selection-2026-08-26.pt.md).

## Telemetria limitada por capability (`1.0.4-rc`)

Cada rota atualiza um outcome fixo e histogramas fixos de latência completa,
espera do worker, espera do lock do tenant e bytes do payload opaco. O snapshot
do processo também informa inflight/pico, pico de fila, reconnects, retries
explícitos, falhas de autenticação, rejeições unhealthy/capacity, pico inflight
por worker, overflow e falhas de exporter. Percentis são limites superiores
dos buckets, não amostras mantidas.

O registry mantém 128 labels validados de capability e uma série fixa de
overflow. Ele nunca cria label por tenant, installation, Core, request,
connection, token, payload ou erro dinâmico. `GatewayTelemetryExporter` é uma
fronteira pull: o caller passa explicitamente um snapshot imutável fora dos
locks de roteamento. Falha do exporter incrementa `export_failures` e retorna
somente ao caller; não rejeita nem atrasa rota porque o roteamento não o chama.

A gate de release executa 4.096 rotas rejeitadas instrumentadas e 256 snapshots
na cardinalidade máxima. Os budgets são 1 ms p99 por rota e 5 ms p99 por
snapshot. Adapters Prometheus e OpenTelemetry consomem esse mesmo contrato fora
do crate e possuem suas filas, retry e policy de transporte.
As medições limpas de referência estão no
[benchmark da telemetria Gateway](benchmarks/gateway-telemetry-2026-08-26.pt.md).

**Maturidade:** perfil RC de peer transport V1; telemetria detalhada é contrato
RC atual.
