# appcore-distributed-contracts

Release candidate **`appcore-distributed-contracts 1.0.2-rc.1`**. Selecionar esta RC explicitamente; não substitui versões estáveis anteriores nem certifica um deployment. Quando existem, as dependências AppCore internas estão fixadas nesta rodada RC.


Testes locais:

```bash
cargo test -p appcore-distributed-contracts
```

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

`OpaqueEnvelopeDeduplicator` retém uma única alocação compartilhada por ID de
mensagem aceito entre seus índices de membership e ordem de aceitação. Eviction
FIFO limitada e decisões de duplicata não mudam. Reter 65.536 IDs distintos de
128 bytes mediu 32,83 ms p50 e 27,25 MiB de RSS pico no Apple M1, contra
37,55 ms e 35,86 MiB com strings duplicadas. Validação de transporte e
deduplicação rejeitam IDs vazios, caracteres de controle e IDs acima de
`MAX_OPAQUE_MESSAGE_ID_BYTES` (1.024 bytes UTF-8) antes da retenção.

Peer RPC V2 é uma família separada e opt-in de frames em `peer_rpc::v2`.
Frames open, chunk, commit e cancel declaram protocolo, identidade, sequência,
tamanhos decodificados, deadline e integridade exatos. Bytes codificados usam
uma string JSON base64 canônica, nunca array de inteiros. O encode legível emite
essa string com buffers scratch fixos de 3 KiB de entrada e 4 KiB de saída; o
decode empresta a string JSON codificada quando o deserializador permite. O
wire exato não muda. V1 permanece somente em `peer_rpc::v1`; implementações
nunca podem inferir ou converter entre versões.

V2 também define um codec binário selecionado explicitamente. Magic fixo,
versão do codec, tipo da mensagem e tamanho exato envolvem um payload Postcard
limitado; bytes de chunk continuam nativos em vez de base64. O JSON não muda,
e frame ou reply binário é limitado a 256 KiB antes do decode. Mismatch de
codec é erro, nunca fallback automático.

Rejeições V2 usam `PeerRpcWireErrorV2`: code fixo, phase e retryability
autoritativos, retry hint/correlation limitados e mensagem redigida controlada
pelo protocolo. Code desconhecido vira o único resultado terminal `unknown`.
A rejeição string estável do V1 possui decoder exato separado e nunca usa
comparação por substring.

**Maturidade:** V1 estável; contrato de chunks V2 em desenvolvimento pós-1.0.

## Documentação estável

ID estável: **ACR-006**. Consulte o
[guia complementar de arquitetura e integração](https://wiki.appcore.dnettoraw.com/pt/crates/id/acr-006). Esse ID permanente
continua válido se a página da wiki mudar.

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
