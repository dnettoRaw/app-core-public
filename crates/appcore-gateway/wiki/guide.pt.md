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

> **Migração do RC atual:** o acesso direto a
> `GatewayState::tenants` foi removido para que tenants independentes não
> compartilhem um único lock. Código que usa esse campo falha na compilação;
> use `tenant_partition`,
> `tenant_partition_or_insert`, `tenant_count` e `connection_count`. Os mapas
> de requests pendentes agora são privados; use `pending_request_count` para
> observação e deixe o `EnvelopeRouter` controlar seu ciclo. Não existe alias
> de compatibilidade nem mapa-espelho. A migração completa está em
> `release/gateway-tenant-migration.md`.

O diretório privado armazena 32 gerações imutáveis de shards copy-on-write.
Scans de admissão, heartbeat e HA copiam somente esses 32 handles `Arc` e
liberam todos os locks dos shards antes de inspecionar as partições
compartilhadas; nenhuma lista completa de tenants é alocada ou clonada.

O gateway resolve o tenant pelo sufixo de dominio definido pelo deployment ou
por parametro de query usado em teste local, autentica conexoes quando
configurado, roteia envelopes Peer RPC e requests HTTP Peer RPC via mesh relay
somente dentro da particao do tenant e remove workers stale mantendo filas de
saida limitadas.

Um deployment explícito pode declarar a configuração Gateway no mapa de adapters
do Deployment Manifest:

```toml
[adapters.gateway]
provider_id = "appcore-gateway"
settings = { bind_address = "127.0.0.1:8080", domain_suffix = "gateway.example.com", heartbeat_interval_ms = "30000", heartbeat_timeout_ms = "90000" }
secret_refs = {}
```

O manifesto declara configuração; ele não inicia o Gateway. A integração
de deployment passa o provider selecionado a
`GatewayConfig::from_provider_config`, que aceita somente as quatro settings
acima e rejeita endpoints, referências de segredo, settings desconhecidas e
tentativas de desligar a autenticação.

O deployment é responsável pela autorização de capabilities, provider de
segurança, injeção explícita do replay store, registro no Supervisor, startup
e shutdown. `GatewayRuntime::new` cria um runtime parado com proteção de replay
limitada e local ao processo. Para replay após restart ou entre instâncias,
use explicitamente `GatewayRuntime::with_replay_store`; a composição HA usa
`GatewayRuntime::with_ha_coordinator` com store e coordinator.
Um caminho no manifesto não constrói esses objetos. Um arquivo compartilhado
só serve se o filesystem cumprir o contrato de segurança entre processos do
store; um arquivo local não coordena hosts distintos.

O SDK não faz essa composição. O deployment deve propagar configuração
inválida e falhas de startup/bind; apenas declarar configuração não cria
listener nem task.

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
consenso, terminador TLS publico ou gerenciador de segredos de producao. HA tem
um contrato opt-in de provider descrito abaixo. Novos edge relays e transports
alternativos não podem enfraquecer autenticação, expiry, nonce ou replay
protection do Peer RPC.

Embedders usam por padrão um replay store limitado e local ao processo. Para
proteger tokens de conexão contra replay após restart ou entre instâncias, o
deployment deve injetar um `PeerNonceStore` apropriado por
`GatewayState::with_replay_store` ou `GatewayRuntime::with_replay_store`.
`FilePeerNonceStore` é seguro entre processos sob seu contrato de filesystem;
um arquivo local, sozinho, não coordena hosts distintos. O host Runtime removido
não configura mais um store nem `paths.gateway_replay`. Sockets ativos expiram
com suas credenciais em até 60 segundos. Leases HA e request fencing não
substituem a admissão antirreplay dos tokens de conexão. Rate limit por IP e
terminação TLS continuam no deployment.

`GatewayRuntime` possui listener, runtime Tokio current-thread, router, pruner
de heartbeat e thread. O startup faz bind sincronamente, portanto endereco
invalido ou ocupado aborta o host. O shutdown cooperativo limitado faz join de
todo o trabalho. Antes do prazo ele descarta o future do servidor, fechando
conexoes lentas ou incompletas antes do join da thread. `Orphaned` e apenas
quarentena defensiva de falha da thread. Snapshots seguros contem apenas
lifecycle, enderecos de bind e contadores. Usuarios
diretos de `spawn_heartbeat_pruner` devem guardar e aguardar o join handle.

O runtime e o transporte de federação usam no máximo 16 threads blocking e 16
permits antes da admissão, com stacks de 1 MiB e remoção de ociosas em cinco
segundos. Gate cheio cancela o request fence antes de rejeitar a rota.
Um request de federação admitido transfere ownership para o worker blocking e
depois transfere seu buffer JSON codificado para `HttpRequest`; o payload Peer
RPC interno completo não é clonado em nenhuma dessas fronteiras.
A credential externa usa `json_payload_hash` para transmitir o JSON canônico
ao SHA-256; o hashing não retém um segundo body codificado completo.

Hashes de conexão de worker e client usam framing binário canônico V2 e levam
o marcador `v2:`. Hashes anteriores sem versão não são intercambiáveis;
emissores de token e consumidores Gateway devem ser atualizados juntos.
O hashing empresta todos os campos de validação. No limite de 64 capabilities
de 128 bytes, o framing escreve direto no output hexadecimal final de 17 KiB,
sem manter um frame binário de 8,5 KiB e uma segunda string de 17 KiB exclusiva
do hash. O parser possui cada nome validado único uma vez e deduplica com
slices emprestados. Consulte o [benchmark do hash de conexão](benchmarks/gateway-connection-hash-2026-09-03.pt.md).

Cada tenant mantém índices diretos e limitados por Core ID e por
`(cluster_id, core_id)`. O lookup comum com Core único é O(1); Core IDs
duplicados usam scan limitado pelo teto de workers do tenant. Register,
reconnect, disconnect e prune de heartbeat atualizam mapa primário, registry de
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

O build padrao do crate inclui o contrato HA independente de provider, mas nao
inclui cliente Redis, stack TLS do Redis ou owner de credential Redis. O crate
de composicao que selecionar essa integracao deve habilita-la explicitamente:

```toml
appcore-gateway = { version = "1.0.6-rc", features = ["ha-redis"] }
```

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
deadline total de cinco segundos. Renewal compartilha snapshots privados e
imutaveis de leases/workers; nao copia listas completas de ownership antes do
I/O do provider, e mutacoes locais serializadas usam copy-on-write. O loop
cooperativo tenta recovery novamente enquanto isolado e libera leases exatos
depois de fechar admission.

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
First-available, least-inflight e affinity percorrem identidades emprestadas sem
alocar lista de candidatos. Round-robin e health-weighted usam um buffer
compacto emprestado porque sua distribuição ordenada estável exige isso;
somente o resultado público selecionado clona sua chave.
O lookup usa o índice existente por Core sem alocar chaves tuple temporárias de
installation/Core. Se instalações compartilham um Core ID, um scan exato
limitado pelo teto de 1.024 workers por tenant preserva a identidade. Execuções
equivalentes da certificação Gateway completa reduziram allocs em 89,10%, bytes
solicitados em 45,21% e o p99 de seleção entre 15,63% e 20,87%.
As medições limpas de referência estão no
[benchmark de seleção de workers do Gateway](benchmarks/gateway-worker-selection-2026-08-26.pt.md).

O índice reverso compartilha cada nome distinto de capability entre todos os
anúncios de workers do tenant sem mudar o índice direto de roteamento.
`CapabilityRegistry::capabilities_for_iter` expõe nomes estáveis emprestados e
`CapabilityRegistry::stats` expõe contagens de ownership sem payload. Remover o
último anunciante libera o nome compartilhado. A evidência A/B limitada está no
[benchmark do registry de capabilities](benchmarks/gateway-capability-registry-2026-09-03.pt.md).

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
