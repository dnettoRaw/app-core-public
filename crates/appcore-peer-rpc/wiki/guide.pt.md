# appcore-peer-rpc

[Exemplo minimo](examples/basic.pt.md) |
[Exemplo intermediario](examples/intermediate.pt.md)

**Responsabilidade:** client peer autenticado, host HTTP, validação e replay
protection.

**Dependências internas:** core, distributed contracts, security e transport.

**API principal:** traits de token issuer/authenticator/dispatcher e
implementações HashToken/static; nonce stores memória/arquivo; config,
validator e hashes; retry/client config e transport trait; transportes pooled e
standard one-shot; HTTP state e host.

Use `PooledPeerRpcTransport` para reutilizar conexões limitadas por origem.
`StdPeerRpcTransport` preserva o comportamento V1 one-shot com
`Connection: close`.

Use somente quando tenant, cluster, source, target, protocolo, expiry, nonce e
integridade podem ser provados. `AllowPeerAuthenticator` é somente teste.

O `Debug` dos DTOs peer request, response, outbound e HTTP mostra tamanhos e
omite bytes opacos, credenciais, valores de nonce/idempotencia e detalhes de
erro remoto.

Com protocolo V2 selecionado explicitamente, `PeerRpcChunkEncoder` lê um chunk
limitado de uma source `Read` e emite frames open/chunk/commit;
`PeerRpcChunkAssembler` verifica e escreve um chunk decodificado em sink
`Write`. O limite agregado default é 64 MiB. Input ausente, duplicado, fora de
ordem, corrompido, expandido acima da quota, expirado ou cancelado fecha o
assembler permanentemente. Finish com falha descarta o sink sem expor bytes
parciais como commitados.

`PeerRpcStreamRegistry` controla sessões V2 parciais com quotas explícitas de
sessões e bytes decodificados. Requisições usam arquivos exclusivos em um
diretório de spool existente e acessível somente pelo proprietário; apenas
payloads totalmente verificados chegam ao dispatcher e respostas usam pulls
explícitos e limitados. Erro, cancelamento, expiração e conclusão removem o
arquivo e a reserva. O snapshot informa sessões, bytes reservados, saturações e
limpezas.
Unix valida o proprietário efetivo e modos `0700`/`0600` do diretório/arquivo.
Windows rejeita reparse points e todo allow ACE fora do SID proprietário do
processo atual. Outras plataformas falham fechadas ao construir o registry.

Instale HTTP V2 explicitamente com
`PeerRpcHttpHost::with_v2_stream_registry`. O host default continua V1-only e
V2 usa JSON canônico por default. Framing binário exige o opt-in separado
`with_v2_binary_codec` no host e
`with_stream_codec_v2(PeerRpcStreamCodecV2::Binary)` no client. Ele usa paths
query/command distintos e o media type exato
`application/vnd.appcore.peer-rpc.v2+postcard`. Cada body selecionado exato é
autenticado e request/response avança um frame por vez. Bodies binários nunca
recebem gzip HTTP e permanecem abaixo de 256 KiB; gzip opcional do chunk ainda
é decodificado sob o limite declarado. Rota ausente, mismatch de media type ou
reply malformado é terminal, sem fallback JSON. A admissão do open valida tenant, cluster,
target, trace, deadline, idempotência de command e nonce replay. Frames nunca
são repetidos após falha ambígua de transporte; cancelamento é best effort e a
limpeza por deadline é autoritativa.

Bodies de rejeição V2 usam `PeerRpcWireErrorV2`. O client valida code, phase,
retryability, retry delay, correlation e mensagem controlada pelo protocolo
como uma única matriz antes de retornar
`PeerRpcStreamClientErrorV2::Remote`. Codes desconhecidos são observáveis,
porém terminais e redigidos. Rejeições V1 viram
`PeerRpcError::RemoteRejected` por igualdade exata; disponibilidade e
capacidade de replay são os únicos casos V1 remotos com retry. Nenhum caminho
interpreta substring.

Disponibilidade do codec V2 não é negociação. O caller deve selecionar módulo
e transporte V2 explicitamente. `/v1/peer/*` interpreta somente V1 e não faz
fallback automático.

**Maturidade:** V1 estável; transporte V2 pós-1.0 certificado em desenvolvimento.

[Evidência de certificação do stream V2 limitado](benchmarks/peer-rpc-v2-2026-08-26.pt.md)
