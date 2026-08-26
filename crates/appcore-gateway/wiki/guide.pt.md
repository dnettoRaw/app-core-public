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

> **Migração do próximo major:** o acesso direto a
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

**Maturidade:** perfil RC de peer transport para a superficie distribuida V1.
