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
consenso, terminador TLS publico ou gerenciador de segredos de producao. HA do
gateway, federacao de edge relays e transports alternativos continuam trabalho
futuro e nao podem enfraquecer autenticacao, expiry, nonce ou replay protection
do Peer RPC.

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

**Maturidade:** perfil RC de peer transport para a superficie distribuida V1.
