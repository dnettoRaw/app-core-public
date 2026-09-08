# appcore-distributed-contracts

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
