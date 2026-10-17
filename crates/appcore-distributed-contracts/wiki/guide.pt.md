# appcore-distributed-contracts

Release candidate **`appcore-distributed-contracts 1.0.2-rc.1`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.


[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** contratos wire/provider versionados de control plane e
peer RPC.

**Dependências internas:** `appcore-contracts`, `appcore-types`.

**API principal:** constantes e paths do protocolo, registration, presence,
heartbeat, peer directory, leases de compatibilidade, leases por serviço,
leadership decisions e traits; paths peer, envelopes, responses, errors, call
kinds, advertisement DTOs, client executor e metadados de transporte para
content-envelope opaco.

Implementações pertencem aos crates de control plane ou peer. Não adicione
cliente HTTP, filesystem, tokens ou regras de capability de produto.

A serializacao wire de opaque-content e Peer RPC nao muda. O `Debug` mostra
tamanhos e metadata de roteamento, sem bytes de payload opaco, valores de
nonce/idempotencia ou detalhes de erro remoto.

`OpaqueEnvelopeDeduplicator` guarda uma alocação compartilhada para cada ID de
mensagem aceito entre o set de membership e a fila de ordem de aceitação. API
pública, resultado de duplicata e eviction FIFO limitada não mudam. Com 65.536
IDs distintos de 128 bytes no Apple M1, p50 caiu de 37,55 ms para 32,83 ms e
RSS pico de 35,86 MiB para 27,25 MiB. Tanto `validate_transport` quanto `accept`
rejeitam IDs vazios, caracteres de controle e IDs acima de
`MAX_OPAQUE_MESSAGE_ID_BYTES` (1.024 bytes UTF-8). A rejeição ocorre antes da
alocação ou eviction FIFO.

`peer_rpc::v2` é uma família independente e opt-in de frames. O frame open fixa
quota agregada, tamanho/quantidade de chunks e deadline; chunks carregam
sequência, encoding, tamanho decodificado e digest exatos; commit vincula
tamanho e digest totais; cancel usa motivo controlado. Bytes codificados usam
string JSON base64 canônica, não array de inteiros. V1 e V2 possuem módulos
e rotas separados. O encode legível emite base64 com buffers scratch fixos de
3 KiB de entrada e 4 KiB de saída. O decode empresta a string JSON codificada
quando o deserializador permite e então aloca somente os bytes decodificados
necessários. Fixtures exatas não mudam. Nenhum parser detecta, atualiza ou faz
fallback entre versões.

O codec binário V2 opcional é uma representação separada e selecionada
explicitamente. O marcador fixo `APCRPC2B`, versão do codec, tipo frame/reply e
tamanho exato vinculam um payload Postcard limitado. Serializadores não humanos
carregam chunks como bytes nativos; a representação JSON existente continua
base64 canônica. Encode e decode recebem o limite do chamador e sempre aplicam
o teto de protocolo de 256 KiB. Mismatch de tipo, marcador, versão, tamanho ou
codec falha antes de alcançar uma implementação.

`PeerRpcWireErrorV2` carrega metadata fixa `code`, `phase` e `retryable`,
`retry_after_ms` e `correlation_id` opcionais e limitados, e mensagem redigida
exata. O decode valida toda a matriz. Metadata conhecida contraditória é
inválida; code desconhecido descarta mensagem/hint e vira `unknown` terminal.
Separadamente, `PeerRpcRemoteErrorV1` decodifica apenas strings V1 estáveis
exatas, então texto remoto livre não seleciona retry.

**Maturidade:** V1 estável; contrato de chunks V2 pós-1.0 em desenvolvimento.

## Projeção de diagnóstico operacional

Esta secção descreve alterações ainda não publicadas do código-fonte. Verifique o pacote exato do registry antes da adoção pelos consumidores.

`peer_rpc::v2::PeerRpcDiagnosticV2::from_wire` valida a matriz de erros existente antes de conservar código, fase, retry e correlação com um `CoreId` confiável. `PeerRpcDiagnosticActionV2` fornece uma sugestão controlada; códigos desconhecidos são redigidos e não permitem retry. Autentique e filtre a exportação por tenant; nunca repita automaticamente um command ambíguo. O schema wire existente não muda; este contrato não depende de implementações.

## Candidato de release — 2026-10-14

`2.0.0-alpha.4`: alterações próprias.


## Autoridade exata por serviço — fonte não publicada

`ServiceLeaseAuthority` é um contrato aditivo em `appcore-distributed-contracts::control_plane::lease_authority`. Aquisição, renovação exata, libertação exata, inspeção e check usam relógio servidor. `FileControlPlane` implementa TTL de 1–30000 ms: adquirir para o mesmo Core devolve a concessão viva sem prolongá-la; renovar não readquire uma concessão expirada e versões alteradas são recusadas. Escritor de dados e coordenador de updates usam `ServiceId`s distintos escolhidos pelo deployment. Heartbeat, papel e épocas HA de instâncias Gateway não conferem autoridade de escrita de dados.

No restart usar `FileControlPlane::open_existing_with_clock`: recusa estado ausente/corrupto e nunca inicializa novo domínio de épocas. Provisionamento inicial por `with_clock` é uma operação explícita. As operações de autoridade usam lock OS sem espera, reportando contenção/indisponibilidade. `with_fenced_service_write` verifica a concessão exata e executa um commit local confiável e limitado sob o mesmo lock; takeover não se sobrepõe ao commit mesmo se a concessão expirar durante ele. O callback não pode reentrar no árbitro; o host possui limites de I/O e idempotência da aplicação. Um check remoto é apenas observação: o storage remoto deve impor fencing atomicamente no commit. Não guardar um check bem-sucedido como permissão offline.

Gateway acrescenta composição explícita `make_service_lease_router(gateway, authority, replay)`, ausente do router default. POST `/v1/gateway/service-lease` recebe `ServiceLeaseRequestV1` (version=1, identity, service_id, action); ações acquire/renew/release/inspect/check. Autenticação obrigatória também em insecure-testing. Domínio isolado `service_lease_token_claims` exige nome command/query exato, subject Core, hash do pedido completo e JTI de uso único até 96 bytes. A credencial não pode durar mais do que a aquisição pedida ou, nas ações com grant, que a expiração da concessão. Tokens relay, alteração de scope/ação/época e coringas não autorizam. O emissor deve autorizar tenant/cluster/Core/serviço/ação antes de assinar; clientes não recebem o segredo de assinatura.

Compor `PeerNonceStore` durável dedicado e partilhado entre restarts/réplicas do árbitro, por exemplo `FilePeerNonceStore` em storage certificado. Não há default inferido. Pedidos até 16 KiB, oito operações admitidas sem fila de admissão. Autenticação/replay I/O e chamadas do árbitro executam em workers bloqueantes fora da thread de polling async. Deployment possui deadlines I/O dos providers, drenagem, TLS, relógio partilhado e custódia dos estados. Respostas: 401 sem credencial, 403 inválida/repetida, 409 concessão alterada/conflito e 503 autoridade indisponível/em contenção ou saturação. Rejeição nunca vira sucesso offline. Falha do provider ou resposta perdida é ambígua: consultar com nova credencial, sem adivinhar sucesso nem repetir uma escrita com nova época automaticamente.

Limites: Core identifica o escritor, não o PID; partilhar credenciais Core partilha autoridade. Provisionar Cores distintos para escritores concorrentes e serviço lógico estável para descoberta. Referência não é consenso, árbitro HA com discos independentes nem fencing de storage remoto. Perda/rollback total de estado do árbitro ou replay requer recuperação controlada/novo domínio; apagar ficheiros ou usar o construtor inicializador não é atalho de recuperação. Wire/storage V1 e APIs anteriores mantidos. Integração no consumidor, publicação registry e qualificação DNET/partição física das duas máquinas continuam pendentes.
